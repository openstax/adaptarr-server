use proc_macro2::{TokenStream, Span};
use syn::{Attribute, Ident, Meta, MetaList, NestedMeta, Lit, LitStr, spanned::Spanned};
use synstructure::{BindingInfo, Structure, VariantInfo};

#[derive(Debug)]
struct Error(TokenStream);

impl Error {
    fn new(span: Span, message: &str) -> Error {
        Error(quote_spanned! { span =>
            compile_error!(#message);
        })
    }

    fn into_tokens(self) -> TokenStream {
        self.0
    }
}

pub fn derive_error(s: Structure) -> TokenStream {
    let statuses = s.each_variant(|v| match find_status(v) {
        Ok(v) => v,
        Err(e) => e.into_tokens(),
    });

    let codes = s.each_variant(|v| match find_code(v) {
        Ok(v) => v,
        Err(e) => e.into_tokens(),
    });

    let data = s.each_variant(|v| match find_extra_data(v) {
        Ok(v) => v,
        Err(e) => e.into_tokens(),
    });

    s.gen_impl(quote! {
        use std::borrow::Cow;
        use adaptarr_error::_reexports::*;

        gen impl ApiError for @Self {
            fn status(&self) -> StatusCode {
                match *self { #statuses }
            }

            fn code(&self) -> Option<Cow<str>> {
                match *self { #codes }
            }

            fn data(&self) -> Option<Value> {
                match *self { #data }
            }
        }
    })
}

/// Given a list of attributes find `#[api(...)]`, and ensure there is only one
/// of them.
fn find_api(attrs: &[Attribute]) -> Result<Option<MetaList>, Error> {
    let mut attrs = attrs.iter()
        .filter_map(|attr| attr.parse_meta().ok())
        .filter(|meta| meta.path().is_ident("api"));

    let meta = match attrs.next() {
        Some(meta) => meta,
        None => return Ok(None),
    };

    let meta = match meta {
        Meta::List(meta) => meta,
        _ => return Err(Error::new(
            meta.span(),
            "api attribute must take a list in parentheses",
        ))
    };

    if meta.nested.is_empty() {
        return Err(Error::new(
            meta.span(),
            "api attribute requires at least one argument",
        ));
    }

    if let Some(meta) = attrs.next() {
        return Err(Error::new(
            meta.span(),
            "api attribute must be used exactly once",
        ));
    }

    Ok(Some(meta))
}

/// Find value of [`ApiError::status()`] for a variant.
fn find_status(v: &VariantInfo) -> Result<TokenStream, Error> {
    let meta = match find_api(v.ast().attrs)? {
        Some(meta) => meta,
        None => return v.bindings()
            .iter()
            .find(is_cause)
            .map(|cause| quote!(#cause.status()))
            .ok_or_else(|| Error::new(
                v.ast().ident.span(),
                "each variant must be #[api]-annotated or have a #[cause]",
            )),
    };

    let mut internal = None;
    let mut status = None;

    for item in meta.nested {
        match item {
            NestedMeta::Meta(Meta::Path(ref path)) if path.is_ident("internal") =>
                internal = Some(item),
            NestedMeta::Meta(Meta::NameValue(ref nv)) if nv.path.is_ident("code") => (),
            NestedMeta::Meta(Meta::NameValue(ref nv)) if nv.path.is_ident("status") =>
                status = Some(match nv.lit {
                    Lit::Str(ref s) => Ident::new(&s.value(), s.span()),
                    _ => return Err(Error::new(
                        nv.lit.span(),
                        "expected a string",
                    )),
                }),
            _ => return Err(Error::new(
                item.span(),
                "expected one of: internal, code, status",
            )),
        }
    }

    if let Some(status) = status {
        if let Some(item) = internal {
            Err(Error::new(item.span(), "internal errors can't have statuses"))
        } else {
            Ok(quote!(StatusCode::#status))
        }
    } else {
        Ok(quote!(StatusCode::INTERNAL_SERVER_ERROR))
    }
}

/// Find value of [`ApiError::code()`] for a variant.
fn find_code(v: &VariantInfo) -> Result<TokenStream, Error> {
    let meta = match find_api(v.ast().attrs)? {
        Some(meta) => meta,
        None => return v.bindings()
            .iter()
            .find(is_cause)
            .map(|cause| quote!(#cause.code()))
            .ok_or_else(|| Error::new(
                v.ast().ident.span(),
                "each variant must be #[api]-annotated or have a #[cause]",
            )),
    };

    let mut internal = None;
    let mut code = None;

    for item in meta.nested {
        match item {
            NestedMeta::Meta(Meta::Path(ref path)) if path.is_ident("internal") =>
                internal = Some(item),
            NestedMeta::Meta(Meta::NameValue(ref nv)) if nv.path.is_ident("code") =>
                code = Some(nv.lit.clone()),
            NestedMeta::Meta(Meta::NameValue(ref nv)) if nv.path.is_ident("status") => (),
            _ => return Err(Error::new(
                item.span(),
                "expected one of: internal, code, status",
            )),
        }
    }

    if let Some(code) = code {
        if let Some(item) = internal {
            Err(Error::new(item.span(), "internal errors can't have codes"))
        } else {
            Ok(quote!(Some(Cow::Borrowed(#code))))
        }
    } else {
        Ok(quote!(None))
    }
}

/// Given a list of attributes find `#[extra(...)]`, and ensure there is only
/// one of them.
fn find_data(attrs: &[Attribute]) -> Result<Option<Meta>, Error> {
    let mut attrs = attrs.iter()
        .filter_map(|attr| attr.parse_meta().ok())
        .filter(|meta| meta.path().is_ident("data"));

    let meta = match attrs.next() {
        Some(meta) => meta,
        None => return Ok(None),
    };

    if let Some(meta) = attrs.next() {
        return Err(Error::new(
            meta.span(),
            "#[data] may be used at most once per field",
        ));
    }

    Ok(Some(meta))
}

fn find_extra_data(v: &VariantInfo) -> Result<TokenStream, Error> {
    if find_api(v.ast().attrs)?.is_none() {
        return v.bindings()
            .iter()
            .find(is_cause)
            .map(|cause| quote!(#cause.data()))
            .ok_or_else(|| Error::new(
                v.ast().ident.span(),
                "each variant must be #[api]-annotated or have a #[cause]",
            ));
    }

    let mut fields = Vec::new();

    for bi in v.bindings() {
        let ast = bi.ast();
        let meta = match find_data(&ast.attrs)? {
            Some(meta) => meta,
            None => continue,
        };

        let span = meta.span();
        let mut key = None;

        match meta {
            Meta::Path(_) => match &ast.ident {
                Some(ref id) => key = Some(LitStr::new(&id.to_string(), id.span())),
                None => (),
            },
            Meta::NameValue(nv) => match nv.lit {
                Lit::Str(lit) => key = Some(lit),
                lit => return Err(Error::new(lit.span(), "expected string")),
            },
            Meta::List(ref list) => return Err(
                Error::new(list.paren_token.span, "expected `=`")),
        }

        fields.push((span, key, bi.binding.clone()));
    }

    if fields.is_empty() {
        Ok(quote!(None))
    } else if fields.len() == 1 {
        let (_, key, field) = fields.pop().unwrap();

        Ok(match key {
            Some(key) => quote!(Some({
                let mut map = Map::default();
                map.insert(
                    #key.to_string(),
                    to_value(#field).expect("serialization failed"),
                );
                map.into()
            })),
            None => quote!(Some(to_value(#field).expect("serialization failed"))),
        })
    } else {
        let values = fields.into_iter()
            .map(|(span, key, field)| match key {
                None => Error::new(
                    span, "there can be at most one unnamed field").into_tokens(),
                Some(key) => quote_spanned! {span=>
                    map.insert(
                        #key.to_string(),
                        to_value(#field).expect("serialization failed"),
                    );
                },
            });

        Ok(quote!(Some({
            let mut map = Map::default();
            #(#values)*
            map
        })))
    }
}

fn is_cause(bi: &&BindingInfo) -> bool {
    bi.ast()
        .attrs
        .iter()
        .filter_map(|attr| attr.parse_meta().ok())
        .any(|meta| meta.path().is_ident("cause"))
}
