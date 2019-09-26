use proc_macro2::TokenStream;
use synstructure::Structure;

pub fn derive_from(s: Structure) -> TokenStream {
    let v = &s.variants()[0];

    let final_value = v.construct(|field, _| {
        let name = field.ident.as_ref().unwrap();
        quote! {
            state.#name
                .or_else(FromField::default)
                .ok_or(MultipartError::FieldMissing(stringify!(#name)))?
        }
    });

    let mut state_fields = Vec::new();
    let mut initial_state = Vec::new();
    let mut arms = Vec::new();

    for binding in v.bindings() {
        let name = binding.ast().ident.as_ref().unwrap();
        let ty = &binding.ast().ty;

        state_fields.push(quote!(#name: Option<#ty>));

        initial_state.push(quote!(#name: None));

        arms.push(quote! {
            stringify!(#name) => Box::new(<#ty as FromField>::from_field(body)
                .map(|value| {
                    state.#name = Some(value);
                    state
                }))
        })
    }

    s.gen_impl(quote! {
        use adaptarr_web::multipart::{FromField, FromMultipart, MultipartError};
        use bytes::Bytes;
        use futures::{Future, Stream, future};

        struct State {
            #(#state_fields),*
        }

        gen impl FromMultipart for @Self {
            type Error = MultipartError;
            type Result = Box<dyn Future<
                Item = Self,
                Error = Self::Error,
            >>;

            fn from_multipart<S, F>(fields: S) -> Self::Result
            where
                S: Stream<
                    Item = (String, F),
                    Error = MultipartError,
                > + 'static,
                F: Stream<
                    Item = Bytes,
                    Error = MultipartError,
                > + 'static,
            {
                let state = State { #(#initial_state),* };

                Box::new(
                    fields.fold::<_, _, Box<dyn Future<Item = State, Error = MultipartError>>>(
                        state,
                        |mut state, (name, body)| {
                            match name.as_str() {
                                #(#arms,)*
                                _ => Box::new(future::err(
                                    MultipartError::UnexpectedField(name))),
                            }
                        },
                    )
                    .map(|state| Ok(#final_value))
                    .and_then(future::result)
                )
            }
        }
    })
}
