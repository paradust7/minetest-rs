extern crate proc_macro2;
extern crate quote;
extern crate syn;

use proc_macro2::Ident;
use proc_macro2::Literal;
use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use quote::ToTokens;
use syn::parse_macro_input;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::Data;
use syn::DeriveInput;
use syn::Field;
use syn::Generics;
use syn::Index;
use syn::Type;
use syn::TypeParam;

#[proc_macro_derive(MinetestSerialize, attributes(wrap))]
pub fn minetest_serialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let serialize_body = make_serialize_body(&name, &input.data);

    // The struct must include Serialize in the bounds of any type
    // that need to be serializable.
    let impl_generic = input.generics.to_token_stream();
    let name_generic = strip_generic_bounds(&input.generics).to_token_stream();
    let where_generic = input.generics.where_clause;

    let expanded = quote! {
        impl #impl_generic Serialize for #name #name_generic #where_generic {
            type Input = Self;
            fn serialize<S: Serializer>(value: &Self::Input, ser: &mut S) -> SerializeResult {
                #serialize_body
                Ok(())
            }
        }
    };
    proc_macro::TokenStream::from(expanded)
}

#[proc_macro_derive(MinetestDeserialize, attributes(wrap))]
pub fn minetest_deserialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let deserialize_body = make_deserialize_body(&name, &input.data);

    // The struct must include Deserialize in the bounds of any type
    // that need to be serializable.
    let impl_generic = input.generics.to_token_stream();
    let name_generic = strip_generic_bounds(&input.generics).to_token_stream();
    let where_generic = input.generics.where_clause;

    let expanded = quote! {
        impl #impl_generic Deserialize for #name #name_generic #where_generic {
            type Output = Self;
            fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
                #deserialize_body
            }
        }
    };
    proc_macro::TokenStream::from(expanded)
}

fn get_wrapped_type(f: &Field) -> Type {
    let mut ty = f.ty.clone();
    for attr in f.attrs.iter() {
        if attr.path.is_ident("wrap") {
            ty = attr.parse_args::<Type>().unwrap();
        }
    }
    ty
}

/// For struct, fields are serialized/deserialized in order.
/// For enum, tags are assumed u8, consecutive, starting with 0.
fn make_serialize_body(input_name: &Ident, data: &Data) -> TokenStream {
    match *data {
        syn::Data::Struct(ref data) => match data.fields {
            syn::Fields::Named(ref fields) => {
                let recurse = fields.named.iter().map(|f| {
                    let name = &f.ident;
                    let ty = get_wrapped_type(f);
                    quote_spanned! {f.span() =>
                        <#ty as Serialize>::serialize(&value.#name, ser)?;
                    }
                });
                quote! {
                    #(#recurse)*
                }
            }
            syn::Fields::Unnamed(ref fields) => {
                let recurse = fields.unnamed.iter().enumerate().map(|(i, f)| {
                    let index = Index::from(i);
                    let ty = get_wrapped_type(f);
                    quote_spanned! {f.span() =>
                        <#ty as Serialize>::serialize(&value.#index, ser)?;
                    }
                });
                quote! {
                    #(#recurse)*
                }
            }
            syn::Fields::Unit => {
                quote! {}
            }
        },
        syn::Data::Enum(ref body) => {
            let recurse = body.variants.iter().enumerate().map(|(i, v)| {
                if !v.fields.is_empty() {
                    quote_spanned! {v.span() =>
                        compile_error!("Cannot handle fields yet");
                    }
                } else if v.discriminant.is_some() {
                    quote_spanned! {v.span() =>
                        compile_error!("Cannot handle discrimiant yet");
                    }
                } else {
                    let id = &v.ident;
                    let i = Literal::u8_unsuffixed(i as u8);
                    quote_spanned! {v.span() =>
                        #id => #i,
                    }
                }
            });
            quote! {
                    use #input_name::*;
                    let tag = match value {
                        #(#recurse)*
                    };
                    u8::serialize(&tag, ser)?;
            }
        }
        syn::Data::Union(_) => unimplemented!(),
    }
}

fn make_deserialize_body(input_name: &Ident, data: &Data) -> TokenStream {
    match *data {
        syn::Data::Struct(ref data) => {
            let inner = match data.fields {
                syn::Fields::Named(ref fields) => {
                    let recurse = fields.named.iter().map(|f| {
                        let name = &f.ident;
                        let ty = get_wrapped_type(f);
                        quote_spanned! {f.span() =>
                            #name: <#ty as Deserialize>::deserialize(deser)?,
                        }
                    });
                    quote! {
                        #(#recurse)*
                    }
                }
                syn::Fields::Unnamed(ref fields) => {
                    let recurse = fields.unnamed.iter().enumerate().map(|(i, f)| {
                        let index = Index::from(i);
                        let ty = get_wrapped_type(f);
                        quote_spanned! {f.span() =>
                            #index: <#ty as Deserialize>::deserialize(deser)?,
                        }
                    });
                    quote! {
                        #(#recurse)*
                    }
                }
                syn::Fields::Unit => {
                    quote! {}
                }
            };
            quote! {
                Ok(Self {
                    #inner
                })
            }
        }
        syn::Data::Enum(ref body) => {
            let recurse = body.variants.iter().enumerate().map(|(i, v)| {
                if !v.fields.is_empty() {
                    quote_spanned! {v.span() =>
                        compile_error!("Cannot handle fields yet");
                    }
                } else if v.discriminant.is_some() {
                    quote_spanned! {v.span() =>
                        compile_error!("Cannot handle discrimiant yet");
                    }
                } else {
                    let id = &v.ident;
                    let i = Literal::u8_unsuffixed(i as u8);
                    quote_spanned! {v.span() =>
                        #i => #id,

                    }
                }
            });

            let input_name_str = Literal::string(&input_name.to_string());
            quote! {
                    use #input_name::*;
                    let tag = u8::deserialize(deser)?;
                    Ok(match tag {
                        #(#recurse)*
                        _ => bail!("Invalid {} tag: {}", #input_name_str, tag),
                    })
            }
        }
        syn::Data::Union(_) => unimplemented!(),
    }
}

/// Converts <T: Trait, S: Trait2> into <T, S>
fn strip_generic_bounds(input: &Generics) -> Generics {
    let input = input.clone();
    Generics {
        lt_token: input.lt_token,
        params: {
            let mut params = input.params.clone();
            params.iter_mut().for_each(|v| {
                *v = match v.clone() {
                    syn::GenericParam::Type(v) => syn::GenericParam::Type(TypeParam {
                        attrs: Vec::new(),
                        ident: v.ident.clone(),
                        colon_token: None,
                        bounds: Punctuated::new(),
                        eq_token: None,
                        default: None,
                    }),
                    any => any,
                }
            });
            params
        },
        gt_token: input.gt_token,
        where_clause: None,
    }
}
