use proc_macro2::TokenStream;
use syn::Attribute;
use synstructure::{BindingInfo, Structure};

pub fn derive_from(s: Structure) -> TokenStream {
    let mut impls = TokenStream::new();

    for variant in s.variants() {
        let from = match variant.bindings()
            .iter()
            .find(is_from)
        {
            Some(from) => from,
            None => continue,
        };

        if variant.bindings().len() > 1 {
            impls.extend(quote_spanned!{variant.ast().ident.span()=>
                compile_error!(
                    "From can only be derived from variants with a single \
                    field");
            });
            continue;
        }

        let ty = &from.ast().ty;

        let constructor = variant.construct(|_, _| quote!(from));

        impls.extend(s.gen_impl(quote! {
            gen impl From<#ty> for @Self {
                fn from(from: #ty) -> Self {
                    #constructor
                }
            }
        }));
    }

    impls
}

fn is_from(bi: &&BindingInfo) -> bool {
    bi.ast()
        .attrs
        .iter()
        .filter_map(Attribute::interpret_meta)
        .any(|meta| meta.name() == "from")
}
