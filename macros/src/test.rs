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
    configs: Vec<Configurator>,
}

#[derive(Debug)]
struct Configurator {
    name: Ident,
    options: Vec<(Ident, Expr)>,
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
    let configs = configure_tests(&opts.configs);

    make_bounds(&mut item.decl);

    Ok(quote_spanned! {item.span()=>
        #[test]
        #vis fn #name() {
            #item

            crate::common::run_test(&#database, #configs, #name);
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

/// Generate code for invoking test configurators.
fn configure_tests(configs: &[Configurator]) -> TokenStream {
    let configs = configs.iter()
        .map(|config| {
            let name = &config.name;
            let name = Ident::new(&format!("configure_{}", name), name.span());
            let options = config.options.iter()
                .map(|(name, value)| quote_spanned!(name.span()=> .#name(#value)));

            quote_spanned! {name.span()=>
                common::#name()
                    #(#options)*
            }
        });

    quote!((#(#configs,)*))
}

mod kw {
    syn::custom_keyword!(database);
}

impl Parse for TestOptions {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut database = None;
        let mut configs = Vec::new();

        while !input.is_empty() {
            if input.parse::<kw::database>().is_ok() {
                input.parse::<Token![=]>()?;
                database = Some(input.parse()?);
            } else {
                configs.push(input.parse()?);
            }

            input.parse::<Option<Token![,]>>()?;
        }

        Ok(TestOptions {
            database,
            configs,
        })
    }
}

impl Parse for Configurator {
    fn parse(input: ParseStream) -> Result<Self> {
        let name = input.parse()?;
        let content;
        syn::parenthesized!(content in input);

        let mut options = Vec::new();

        while !content.is_empty() {
            let option = content.parse()?;
            content.parse::<Token![=]>()?;
            let value = content.parse()?;

            options.push((option, value));

            content.parse::<Option<Token![,]>>()?;
        }

        Ok(Configurator {
            name,
            options,
        })
    }
}
