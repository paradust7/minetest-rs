use super::audit::audit_command;
use super::deser::Deserialize;
use super::deser::DeserializeError;
use super::deser::DeserializeResult;
use super::deser::Deserializer;
use super::ser::Serialize;
use super::ser::SerializeResult;
use super::ser::Serializer;
use super::types::*;
use anyhow::bail;
use std::ops::Deref;

#[macro_export]
macro_rules! as_item {
    ($i:item) => {
        $i
    };
}

#[macro_export]
macro_rules! default_serializer {
    ($spec_ty: ident { }) => {
        impl Serialize for $spec_ty {
            fn serialize<S: Serializer>(&self, _: &mut S) -> SerializeResult {
                Ok(())
            }
        }
    };
    ($spec_ty: ident { $($fname: ident: $ftyp: ty ),+ }) => {
        impl Serialize for $spec_ty {
            fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
                $(
                    Serialize::serialize(&self.$fname, ser)?;
                )+
                Ok(())
            }
        }
    };
}

#[macro_export]
macro_rules! default_deserializer {
    ($spec_ty: ident { }) => {
        impl Deserialize for $spec_ty {
            fn deserialize(_deser: &mut Deserializer) -> DeserializeResult<Self> {
                Ok($spec_ty)
            }
        }
    };
    ($spec_ty: ident { $($fname: ident: $ftyp: ty ),+ }) => {
        impl Deserialize for $spec_ty {
            fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
                Ok($spec_ty {
                    $(
                        $fname: <$ftyp>::deserialize(deser)?,
                    )+
                })
            }
        }
    };
}

#[macro_export]
macro_rules! implicit_from {
    ($command_ty: ident, $name: ident, $spec_ty: ident) => {
        impl From<$spec_ty> for $command_ty {
            fn from(value: $spec_ty) -> Self {
                $command_ty::$name(Box::new(value))
            }
        }
    };
}

#[macro_export]
macro_rules! proto_struct {
    ($spec_ty: ident { }) => {
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $spec_ty;
        $crate::default_serializer!($spec_ty { });
        $crate::default_deserializer!($spec_ty { });
    };
    ($spec_ty: ident {
        $($fname: ident: $ftype: ty ),+
    }) => {
        $crate::as_item! {
            #[derive(Debug, Clone, PartialEq)]
            pub struct $spec_ty {
               $(pub $fname: $ftype),+
            }
        }
        $crate::default_serializer!($spec_ty { $($fname: $ftype),* });
        $crate::default_deserializer!($spec_ty { $($fname: $ftype),* });
    };
}

macro_rules! define_protocol {
    ($version: literal,
     $protocol_id: literal,
     $dir: ident,
     $command_ty: ident => {
         $($name: ident, $id: literal, $channel: literal, $reliable: literal => $spec_ty: ident
             { $($fname: ident : $ftype: ty),* } ),*
    }) => {
        $crate::as_item! {
            #[derive(Debug, PartialEq, Clone)]
            pub enum $command_ty {
                $($name(Box<$spec_ty>)),*,
            }
        }

        $crate::as_item! {
            impl CommandProperties for $command_ty {
                fn direction(&self) -> CommandDirection {
                    CommandDirection::$dir
                }

                fn default_channel(&self) -> u8 {
                    match self {
                        $($command_ty::$name(_) => $channel),*,
                    }
                }

                fn default_reliability(&self) -> bool {
                    match self {
                        $($command_ty::$name(_) => $reliable),*,
                    }
                }

                fn command_name(&self) -> &'static str {
                    match self {
                        $($command_ty::$name(_) => stringify!($name)),*,
                    }
                }
            }
        }

        $crate::as_item! {
            impl Serialize for $command_ty {
                fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
                    match self {
                        $($command_ty::$name(spec) => { u16::serialize(&$id, ser)?; Serialize::serialize(Deref::deref(spec), ser) }),*,
                    }
                }
            }
        }

        $crate::as_item! {
            impl Deserialize for $command_ty {
                fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
                    let orig_buffer = deser.peek_all();
                    let command_id: u16 = Deserialize::deserialize(deser)?;
                    let dir = deser.direction();
                    let result = match (dir, command_id) {
                        $( (CommandDirection::$dir, $id) => $command_ty::$name(Box::new(Deserialize::deserialize(deser)?)) ),*,
                        _ => bail!(DeserializeError::BadPacketId(dir, command_id)),
                    };
                    audit_command(deser.context(), orig_buffer, &result);
                    Ok(result)
                }
            }
        }

        $($crate::proto_struct!($spec_ty { $($fname: $ftype),* });)*
        $($crate::implicit_from!($command_ty, $name, $spec_ty);)*

    };
}

define_protocol!(41, 0x4f457403, ToClient, ToClientCommand => {
    // CommandName, CommandType, Direction, Channel, Reliable
    Hello, 0x02, 0, true => HelloSpec {
        serialization_ver: u8,
        compression_mode: u16,
        proto_ver: u16,
        auth_mechs: AuthMechsBitset,
        username_legacy: String
    },

    AuthAccept, 0x03, 0, true => AuthAcceptSpec {
        player_pos: v3f,
        map_seed: u64,
        recommended_send_interval: f32,
        sudo_auth_methods: u32
    },

    AcceptSudoMode, 0x04, 0, true => AcceptSudoModeSpec {
        // No fields
    },

    DenySudoMode, 0x05, 0, true => DenySudoModeSpec {
        // No fields
    },

    AccessDenied, 0x0A, 0, true => AccessDeniedSpec {
        code: AccessDeniedCode
    },

    Blockdata, 0x20, 2, true => BlockdataSpec {
        pos: v3s16,
        block: MapBlock,
        network_specific_version: u8
    },
    Addnode, 0x21, 0, true => AddnodeSpec {
        pos: v3s16,
        node: MapNode,
        keep_metadata: bool
    },

    Removenode, 0x22, 0, true => RemovenodeSpec {
        pos: v3s16
    },

    Inventory, 0x27, 0, true => InventorySpec {
        inventory: Inventory
    },

    TimeOfDay, 0x29, 0, true => TimeOfDaySpec {
        time_of_day: u16,
        time_speed: Option<f32>
    },

    CsmRestrictionFlags, 0x2A, 0, true => CsmRestrictionFlagsSpec {
        csm_restriction_flags: u64,
        csm_restriction_noderange: u32
    },

    PlayerSpeed, 0x2B, 0, true => PlayerSpeedSpec {
        added_vel: v3f
    },

    MediaPush, 0x2C, 0, true => MediaPushSpec {
        raw_hash: String,
        filename: String,
        cached: bool,
        token: u32
    },

    TCChatMessage, 0x2F, 0, true => TCChatMessageSpec {
        version: u8,
        message_type: u8,
        sender: WString,
        message: WString,
        timestamp: u64
    },

    ActiveObjectRemoveAdd, 0x31, 0, true => ActiveObjectRemoveAddSpec {
        removed_object_ids: Array16<u16>,
        added_objects: Array16<AddedObject>
    },

    ActiveObjectMessages, 0x32, 0, true => ActiveObjectMessagesSpec {
        objects: Array0<ActiveObjectMessage>
    },

    Hp, 0x33, 0, true => HpSpec {
        hp: u16,
        damage_effect: Option<bool>
    },

    MovePlayer, 0x34, 0, true => MovePlayerSpec {
        pos: v3f,
        pitch: f32,
        yaw: f32
    },

    AccessDeniedLegacy, 0x35, 0, true => AccessDeniedLegacySpec {
        reason: WString
    },

    Fov, 0x36, 0, true => FovSpec {
        fov: f32,
        is_multiplier: bool,
        transition_time: Option<f32>
    },

    Deathscreen, 0x37, 0, true => DeathscreenSpec {
        set_camera_point_target: bool,
        camera_point_target: v3f
    },

    Media, 0x38, 2, true => MediaSpec {
        num_bunches: u16,
        bunch_index: u16,
        files: Array32<MediaFileData>
    },

    Nodedef, 0x3a, 0, true => NodedefSpec {
        node_def: ZLibCompressed<NodeDefManager>
    },

    AnnounceMedia, 0x3c, 0, true => AnnounceMediaSpec {
        files: Array16<MediaAnnouncement>,
        remote_servers: String
    },

    Itemdef, 0x3d, 0, true => ItemdefSpec {
        item_def: ZLibCompressed<ItemdefList>
    },

    PlaySound, 0x3f, 0, true => PlaySoundSpec {
        server_id: s32,
        spec_name: String,
        spec_gain: f32,
        typ: u8, // 0=local, 1=positional, 2=object
        pos: v3f,
        object_id: u16,
        spec_loop: bool,
        spec_fade: Option<f32>,
        spec_pitch: Option<f32>,
        ephemeral: Option<bool>
    },

    StopSound, 0x40, 0, true => StopSoundSpec {
        server_id: s32
    },

    Privileges, 0x41, 0, true => PrivilegesSpec {
        privileges: Array16<String>
    },

    InventoryFormspec, 0x42, 0, true => InventoryFormspecSpec {
        formspec: LongString
    },

    DetachedInventory, 0x43, 0, true => DetachedInventorySpec {
        name: String,
        keep_inv: bool,
        // These are present if keep_inv is true.
        ignore: Option<u16>,
        contents: Option<Inventory>
    },

    ShowFormspec, 0x44, 0, true => ShowFormspecSpec {
        form_spec: LongString,
        form_name: String
    },

    Movement, 0x45, 0, true => MovementSpec {
        acceleration_default: f32,
        acceleration_air: f32,
        acceleration_fast: f32,
        speed_walk: f32,
        speed_crouch: f32,
        speed_fast: f32,
        speed_climb: f32,
        speed_jump: f32,
        liquid_fluidity: f32,
        liquid_fluidity_smooth: f32,
        liquid_sink: f32,
        gravity: f32
    },

    SpawnParticle, 0x46, 0, true => SpawnParticleSpec {
        data: ParticleParameters
    },

    AddParticlespawner, 0x47, 0, true => AddParticlespawnerSpec {
        legacy: AddParticleSpawnerLegacy
    },

    Hudadd, 0x49, 1, true => HudaddSpec {
        server_id: u32,
        typ: u8,
        pos: v2f,
        name: String,
        scale: v2f,
        text: String,
        number: u32,
        item: u32,
        dir: u32,
        align: v2f,
        offset: v2f,
        world_pos: Option<v3f>,
        size: Option<v2s32>,
        z_index: Option<s16>,
        text2: Option<String>,
        style: Option<u32>
    },

    Hudrm, 0x4a, 1, true => HudrmSpec {
        server_id: u32
    },

    Hudchange, 0x4b, 1, true => HudchangeSpec {
        server_id: u32,
        stat: HudStat
    },

    HudSetFlags, 0x4c, 1, true => HudSetFlagsSpec {
        flags: HudFlags, // flags added
        mask: HudFlags   // flags possibly removed
    },

    HudSetParam, 0x4d, 1, true => HudSetParamSpec {
        value: HudSetParam
    },

    Breath, 0x4e, 0, true => BreathSpec {
        breath: u16
    },

    SetSky, 0x4f, 0, true => SetSkySpec {
        params: SkyboxParams
    },

    OverrideDayNightRatio, 0x50, 0, true => OverrideDayNightRatioSpec {
        do_override: bool,
        day_night_ratio: u16
    },

    LocalPlayerAnimations, 0x51, 0, true => LocalPlayerAnimationsSpec {
        idle: v2s32,
        walk: v2s32,
        dig: v2s32,
        walk_dig: v2s32,
        frame_speed: f32
    },

    EyeOffset, 0x52, 0, true => EyeOffsetSpec {
        eye_offset_first: v3f,
        eye_offset_third: v3f
    },

    DeleteParticlespawner, 0x53, 0, true => DeleteParticlespawnerSpec {
        server_id: u32
    },

    CloudParams, 0x54, 0, true => CloudParamsSpec {
        density: f32,
        color_bright: SColor,
        color_ambient: SColor,
        height: f32,
        thickness: f32,
        speed: v2f
    },

    FadeSound, 0x55, 0, true => FadeSoundSpec {
        sound_id: s32,
        step: f32,
        gain: f32
    },

    UpdatePlayerList, 0x56, 0, true => UpdatePlayerListSpec {
        typ: u8,
        players: Array16<String>
    },

    TCModchannelMsg, 0x57, 0, true => TCModchannelMsgSpec {
        channel_name: String,
        sender: String,
        channel_msg: String
    },

    ModchannelSignal, 0x58, 0, true => ModchannelSignalSpec {
        signal_tmp: u8,
        channel: String,
        // signal == MODCHANNEL_SIGNAL_SET_STATE
        state: Option<u8>
    },

    NodemetaChanged, 0x59, 0, true => NodemetaChangedSpec {
        list: ZLibCompressed<AbsNodeMetadataList>
    },

    SetSun, 0x5a, 0, true => SetSunSpec {
        sun: SunParams
    },

    SetMoon, 0x5b, 0, true => SetMoonSpec {
        moon: MoonParams
    },

    SetStars, 0x5c, 0, true => SetStarsSpec {
        stars: StarParams
    },

    SrpBytesSB, 0x60, 0, true => SrpBytesSBSpec {
         s: BinaryData16,
         b: BinaryData16
    },

    FormspecPrepend, 0x61, 0, true => FormspecPrependSpec {
        formspec_prepend: String
    },

    MinimapModes, 0x62, 0, true => MinimapModesSpec {
        modes: MinimapModeList
    },

    SetLighting, 0x63, 0, true => SetLightingSpec {
        lighting: Lighting
    }
});

define_protocol!(41, 0x4f457403, ToServer, ToServerCommand => {
    /////////////////////////////////////////////////////////////////////////
    // ToServer
    Null, 0x00, 0, false => NullSpec {
        // This appears to be sent before init to initialize
        // the reliable seqnum and peer id.
    },

    Init, 0x02, 1, false => InitSpec {
        serialization_ver_max: u8,
        supp_compr_modes: u16,
        min_net_proto_version: u16,
        max_net_proto_version: u16,
        player_name: String
    },

    Init2, 0x11, 1, true => Init2Spec {
        lang: Option<String>
    },

    ModchannelJoin, 0x17, 0, true => ModchannelJoinSpec {
        channel_name: String
    },

    ModchannelLeave, 0x18, 0, true => ModchannelLeaveSpec {
        channel_name: String
    },

    TSModchannelMsg, 0x19, 0, true => TSModchannelMsgSpec {
        channel_name: String,
        channel_msg: String
    },

    Playerpos, 0x23, 0, false => PlayerposSpec {
        player_pos: PlayerPos
    },

    Gotblocks, 0x24, 2, true => GotblocksSpec {
        blocks: Array8<v3s16>
    },

    Deletedblocks, 0x25, 2, true => DeletedblocksSpec {
        blocks: Array8<v3s16>
    },

    InventoryAction, 0x31, 0, true => InventoryActionSpec {
        action: InventoryAction
    },

    TSChatMessage, 0x32, 0, true => TSChatMessageSpec {
        message: WString
    },

    Damage, 0x35, 0, true => DamageSpec {
        damage: u16
    },

    Playeritem, 0x37, 0, true => PlayeritemSpec {
        item: u16
    },

    Respawn, 0x38, 0, true => RespawnSpec {
        // empty
    },

    Interact, 0x39, 0, true => InteractSpec {
        action: InteractAction,
        item_index: u16,
        pointed_thing: Wrapped32<PointedThing>,
        player_pos: PlayerPos
    },

    RemovedSounds, 0x3a, 2, true => RemovedSoundsSpec {
        ids: Array16<s32>
    },

    NodemetaFields, 0x3b, 0, true => NodemetaFieldsSpec {
        p: v3s16,
        form_name: String,
        // (name, value)
        fields: Array16<Pair<String, LongString>>
    },

    InventoryFields, 0x3c, 0, true => InventoryFieldsSpec {
        client_formspec_name: String,
        fields: Array16<Pair<String, LongString>>
    },

    RequestMedia, 0x40, 1, true => RequestMediaSpec {
        files: Array16<String>
    },

    HaveMedia, 0x41, 2, true => HaveMediaSpec {
        tokens: Array8<u32>
    },

    ClientReady, 0x43, 1, true => ClientReadySpec {
        major_ver: u8,
        minor_ver: u8,
        patch_ver: u8,
        reserved: u8,
        full_ver: String,
        formspec_ver: Option<u16>
    },

    FirstSrp, 0x50, 1, true => FirstSrpSpec {
        salt: BinaryData16,
        verification_key: BinaryData16,
        is_empty: bool
    },

    SrpBytesA, 0x51, 1, true => SrpBytesASpec {
        bytes_a: BinaryData16,
        based_on: u8
    },

    SrpBytesM, 0x52, 1, true => SrpBytesMSpec {
        bytes_m: BinaryData16
    },

    UpdateClientInfo, 0x53, 1, true => UpdateClientInfoSpec {
        render_target_size: v2u32,
        real_gui_scaling: f32,
        real_hud_scaling: f32,
        max_fs_size: v2f
    }
});

#[derive(Debug, PartialEq, Clone)]
pub enum Command {
    ToServer(ToServerCommand),
    ToClient(ToClientCommand),
}

pub trait CommandProperties {
    fn direction(&self) -> CommandDirection;
    fn default_channel(&self) -> u8;
    fn default_reliability(&self) -> bool;
    fn command_name(&self) -> &'static str;
}

pub trait CommandRef: CommandProperties + std::fmt::Debug + Serialize + Deserialize {
    fn toserver_ref(&self) -> Option<&ToServerCommand>;
    fn toclient_ref(&self) -> Option<&ToClientCommand>;
}

impl CommandProperties for Command {
    fn direction(&self) -> CommandDirection {
        match self {
            Command::ToServer(_) => CommandDirection::ToServer,
            Command::ToClient(_) => CommandDirection::ToClient,
        }
    }

    fn default_channel(&self) -> u8 {
        match self {
            Command::ToServer(c) => c.default_channel(),
            Command::ToClient(c) => c.default_channel(),
        }
    }

    fn default_reliability(&self) -> bool {
        match self {
            Command::ToServer(c) => c.default_reliability(),
            Command::ToClient(c) => c.default_reliability(),
        }
    }

    fn command_name(&self) -> &'static str {
        match self {
            Command::ToServer(c) => c.command_name(),
            Command::ToClient(c) => c.command_name(),
        }
    }
}

impl CommandRef for Command {
    fn toserver_ref(&self) -> Option<&ToServerCommand> {
        match self {
            Command::ToServer(s) => Some(s),
            Command::ToClient(_) => None,
        }
    }

    fn toclient_ref(&self) -> Option<&ToClientCommand> {
        match self {
            Command::ToServer(_) => None,
            Command::ToClient(c) => Some(c),
        }
    }
}

impl CommandRef for ToClientCommand {
    fn toserver_ref(&self) -> Option<&ToServerCommand> {
        None
    }

    fn toclient_ref(&self) -> Option<&ToClientCommand> {
        Some(self)
    }
}

impl CommandRef for ToServerCommand {
    fn toserver_ref(&self) -> Option<&ToServerCommand> {
        Some(self)
    }

    fn toclient_ref(&self) -> Option<&ToClientCommand> {
        None
    }
}

impl Serialize for Command {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        match self {
            Command::ToServer(c) => Serialize::serialize(c, ser),
            Command::ToClient(c) => Serialize::serialize(c, ser),
        }
    }
}

impl Deserialize for Command {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(match deser.direction() {
            CommandDirection::ToClient => Command::ToClient(Deserialize::deserialize(deser)?),
            CommandDirection::ToServer => Command::ToServer(Deserialize::deserialize(deser)?),
        })
    }
}
