extern crate proc_macro2;
extern crate quote;
extern crate syn;

use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use quote::ToTokens;
use syn::parse_macro_input;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::Data;
use syn::DeriveInput;
use syn::Generics;
use syn::Index;
use syn::TypeParam;

#[proc_macro_derive(MinetestSerialize)]
pub fn minetest_serialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let serialize_body = make_serialize_body(&input.data);

    // The struct must include Serialize in the bounds of any type
    // that need to be serializable.
    let impl_generic = input.generics.to_token_stream();
    let name_generic = strip_generic_bounds(&input.generics).to_token_stream();

    let expanded = quote! {
        impl #impl_generic Serialize for #name #name_generic {
            fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
                #serialize_body
                Ok(())
            }
        }
    };
    proc_macro::TokenStream::from(expanded)
}

#[proc_macro_derive(MinetestDeserialize)]
pub fn minetest_deserialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let deserialize_body = make_deserialize_body(&input.data);

    // The struct must include Deserialize in the bounds of any type
    // that need to be serializable.
    let impl_generic = input.generics.to_token_stream();
    let name_generic = strip_generic_bounds(&input.generics).to_token_stream();

    let expanded = quote! {
        impl #impl_generic Deserialize for #name #name_generic {
            fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
                Ok(Self {
                    #deserialize_body
                })
            }
        }
    };
    proc_macro::TokenStream::from(expanded)
}

fn make_serialize_body(data: &Data) -> TokenStream {
    match *data {
        syn::Data::Struct(ref data) => match data.fields {
            syn::Fields::Named(ref fields) => {
                let recurse = fields.named.iter().map(|f| {
                    let name = &f.ident;
                    quote_spanned! {f.span() =>
                        Serialize::serialize(&self.#name, ser)?;
                    }
                });
                quote! {
                    #(#recurse)*
                }
            }
            syn::Fields::Unnamed(ref fields) => {
                let recurse = fields.unnamed.iter().enumerate().map(|(i, f)| {
                    let index = Index::from(i);
                    quote_spanned! {f.span() =>
                        Serialize::serialize(&self.#index, ser)?;
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
        syn::Data::Enum(_) => unimplemented!(),
        syn::Data::Union(_) => unimplemented!(),
    }
}

fn make_deserialize_body(data: &Data) -> TokenStream {
    match *data {
        syn::Data::Struct(ref data) => match data.fields {
            syn::Fields::Named(ref fields) => {
                let recurse = fields.named.iter().map(|f| {
                    let name = &f.ident;
                    quote_spanned! {f.span() =>
                        #name: Deserialize::deserialize(deser)?,
                    }
                });
                quote! {
                    #(#recurse)*
                }
            }
            syn::Fields::Unnamed(ref fields) => {
                let recurse = fields.unnamed.iter().enumerate().map(|(i, f)| {
                    let index = Index::from(i);
                    quote_spanned! {f.span() =>
                        #index: Deserialize::deserialize(deser)?,
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
        syn::Data::Enum(_) => unimplemented!(),
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
