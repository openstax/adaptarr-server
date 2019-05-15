extern crate proc_macro;

#[macro_use] extern crate synstructure;

use proc_macro::TokenStream;
use quote::ToTokens;
use syn::parse_macro_input;

mod api;
mod test;

decl_derive!([ApiError, attributes(api)] => api::derive_error);

#[proc_macro_attribute]
pub fn test_database(attr: TokenStream, item: TokenStream) -> TokenStream {
    run_macro(test::create_database, attr, item)
}

#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    run_macro(test::create_test, attr, item)
}

fn run_macro<A, B, R, F>(f: F, attr: TokenStream, item: TokenStream) -> TokenStream
where
    A: syn::parse::Parse,
    B: syn::parse::Parse,
    F: Fn(A, B) -> syn::Result<R>,
    R: ToTokens,
{
    let a = parse_macro_input!(attr as A);
    let b = parse_macro_input!(item as B);

    match f(a, b) {
        Ok(r) => TokenStream::from(r.into_token_stream()),
        Err(err) => TokenStream::from(err.to_compile_error()),
    }
}
