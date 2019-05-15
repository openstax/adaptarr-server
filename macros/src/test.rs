use proc_macro2::{TokenStream, Span};
use std::{
    iter::{self, FromIterator},
    sync::atomic::{AtomicBool, Ordering},
};
use syn::{
    *,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    token::Where,
};

static DB_CREATED: AtomicBool = AtomicBool::new(false);

/// Create a test database.
///
/// This macro, when put on a function
/// `(&Connection) -> Result<(), failure::Error>`, will create and initialize
/// a crate-global test database.
///
/// This function should only be placed on a function in crate's root.
pub fn create_database(_: TokenStream, item: ItemFn) -> Result<TokenStream> {
    let setup = item.ident.clone();

    DB_CREATED.store(true, Ordering::Relaxed);

    Ok(quote! {
        #item

        lazy_static::lazy_static! {
            static ref DATABASE: crate::common::Database =
                crate::common::setup_db(#setup)
                .expect("Cannot create test database");
        }
    })
}

#[derive(Debug)]
pub struct TestOptions {
    database: Option<Path>,
}

/// Build a test case.
///
/// Unlike the standard `#[test]`, this macro accepts additional parameters,
/// which can be used to customise how a test is run, and allows test functions
/// to take parameters of certain types.
pub fn create_test(mut opts: TestOptions, mut item: ItemFn) -> Result<TokenStream> {
    let vis = item.vis.clone();
    let name = item.ident.clone();
    let database = test_database(&mut opts)?;

    make_bounds(&mut item.decl);

    Ok(quote_spanned! {item.span()=>
        #[test]
        #vis fn #name() {
            #item

            crate::common::run_test(&#database, #name);
        }
    })
}

fn test_database(opts: &mut TestOptions) -> Result<TokenStream> {
    match opts.database.take() {
        Some(path) => Ok(quote!(#path)),
        None if DB_CREATED.load(Ordering::Relaxed) => Ok(quote!(crate::DATABASE)),
        None => Err(Error::new(
            Span::call_site(),
            "No test database. Either put a #[adaptarr::test_database] \
            annotated function in crate root, or specify database via \
            #[adaptarr::test(database = path)]",
        )),
    }
}

/// Add where bounds to test functions to ensure `TestResult` and `Fixture` are
/// implemented.
fn make_bounds(decl: &mut FnDecl) {
    let mut path = Punctuated::new();
    path.push(Ident::new("crate", Span::call_site()).into());
    path.push(Ident::new("common", Span::call_site()).into());
    path.push(Ident::new("TestResult", Span::call_site()).into());

    let test_result_path = Path {
        leading_colon: None,
        segments: path.clone(),
    };

    path.pop();
    path.push(Ident::new("Fixture", Span::call_site()).into());

    let fixture_path = Path {
        leading_colon: None,
        segments: path,
    };

    let predicates = match decl.generics.where_clause {
        Some(ref mut clause) => &mut clause.predicates,
        None => {
            decl.generics.where_clause = Some(WhereClause {
                where_token: Where { span: Span::call_site() },
                predicates: Punctuated::new(),
            });
            &mut decl.generics.where_clause.as_mut().unwrap().predicates
        }
    };

    if let ReturnType::Type(_, ref ty) = decl.output {
        predicates.push(PredicateType {
            lifetimes: None,
            bounded_ty: *ty.clone(),
            colon_token: Default::default(),
            bounds: Punctuated::from_iter(iter::once(TraitBound {
                paren_token: None,
                modifier: TraitBoundModifier::None,
                lifetimes: None,
                path: test_result_path,
            }).map(TypeParamBound::from)),
        }.into());
    }

    for arg in &decl.inputs {
        match arg {
            FnArg::SelfRef(_) | FnArg::SelfValue(_) | FnArg::Inferred(_) => {}
            FnArg::Ignored(ty) | FnArg::Captured(ArgCaptured { ty, .. }) => {
                predicates.push(PredicateType {
                    lifetimes: None,
                    bounded_ty: ty.clone(),
                    colon_token: Default::default(),
                    bounds: Punctuated::from_iter(iter::once(TraitBound {
                        paren_token: None,
                        modifier: TraitBoundModifier::None,
                        lifetimes: None,
                        path: fixture_path.clone(),
                    }).map(TypeParamBound::from)),
                }.into());
            }
        }
    }
}

mod kw {
    syn::custom_keyword!(database);
}

impl Parse for TestOptions {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut database = None;

        while !input.is_empty() {
            if input.parse::<kw::database>().is_ok() {
                input.parse::<Token![=]>()?;
                database = Some(input.parse()?);
            } else {
                return Err(input.error("Unexpected token"));
            }

            input.parse::<Option<Token![,]>>()?;
        }

        Ok(TestOptions {
            database,
        })
    }
}
