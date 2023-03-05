use crate::wire::command::Command;
use crate::wire::packet::InnerBody;
use crate::wire::packet::OriginalBody;
use crate::wire::packet::SplitBody;
use crate::wire::packet::MAX_ORIGINAL_BODY_SIZE;
use crate::wire::packet::MAX_SPLIT_BODY_SIZE;
use crate::wire::packet::SEQNUM_INITIAL;
use crate::wire::ser::MockSerializer;
use crate::wire::ser::Serialize;
use crate::wire::ser::VecSerializer;
use crate::wire::types::CommandDirection;

pub struct SplitSender {
    dir: CommandDirection,
    next_seqnum: u64,
}

impl SplitSender {
    pub fn new(remote_is_server: bool) -> Self {
        Self {
            dir: CommandDirection::for_send(remote_is_server),
            next_seqnum: SEQNUM_INITIAL as u64,
        }
    }

    /// Push a Command for transmission
    /// This will possibly split it into 1 or more packets.
    #[must_use]
    pub fn push(&mut self, command: Command) -> anyhow::Result<Vec<InnerBody>> {
        let total_size = {
            let mut ser = MockSerializer::new(self.dir);
            Serialize::serialize(&command, &mut ser)?;
            ser.len()
        };
        let mut result = Vec::new();
        // Packets should serialize to at most 512 bytes
        if total_size <= MAX_ORIGINAL_BODY_SIZE {
            // Doesn't need to be split
            result.push(InnerBody::Original(OriginalBody { command }));
        } else {
            // TODO(paradust): Can this extra allocation be avoided?
            let mut ser = VecSerializer::new(self.dir, total_size);
            Serialize::serialize(&command, &mut ser)?;
            let data = ser.take();
            assert!(data.len() == total_size);
            let mut index: usize = 0;
            let mut offset: usize = 0;
            let total_chunks: usize = (total_size + MAX_SPLIT_BODY_SIZE - 1) / MAX_SPLIT_BODY_SIZE;
            while offset < total_size {
                let end = std::cmp::min(offset + MAX_SPLIT_BODY_SIZE, total_size);
                result.push(InnerBody::Split(SplitBody {
                    seqnum: self.next_seqnum as u16,
                    chunk_count: total_chunks as u16,
                    chunk_num: index as u16,
                    chunk_data: data[offset..end].to_vec(),
                }));
                offset += MAX_SPLIT_BODY_SIZE;
                index += 1;
            }
            assert!(index == total_chunks);
            self.next_seqnum += 1;
        }
        Ok(result)
    }
}
