use adaptarr_macros::From;
use failure::Fail;
use fluent_bundle::{
    FluentError,
    FluentArgs,
    FluentBundle,
    FluentResource,
    FluentValue,
};
use fluent_syntax::parser::errors::ParserError;
use log::error;
use serde::Serialize;
use std::{borrow::Cow, collections::HashMap, fs, fmt::Write};
use unic_langid::errors::LanguageIdentifierError;

use crate::{LanguageTag, LanguageRange};

#[derive(Serialize)]
pub struct Locale {
    pub code: LanguageTag,
    pub name: String,
    #[serde(skip_serializing)]
    messages: FluentBundle<FluentResource>,
}

impl Locale {
    fn new(code: LanguageTag, messages: FluentBundle<FluentResource>)
    -> Result<Self, LoadLocalesError> {
        let (name, errors) = match format(&messages, "locale-name", None) {
            Some(v) => v,
            None => return Err(LoadLocalesError::MissingLocaleName(code)),
        };
        let name = name.into_owned();

        if errors.is_empty() {
            Ok(Locale { code, name, messages })
        } else {
            error!("Could not format message locale-name in locale {}", code);
            Err(LoadLocalesError::InvalidLocaleName(code))
        }
    }

    pub fn format<'a>(&'a self, key: &str, args: &'a HashMap<&'a str, FluentValue<'a>>)
    -> Option<Cow<'a, str>> {
        let (value, errors) = format(&self.messages, key, Some(args))?;

        if errors.is_empty() {
            Some(value)
        } else {
            error!("Could not format message {} in locale {}:{}",
                key, self.code, format_errors(errors.as_slice()));
            None
        }
    }
}

fn format<'a>(
    bundle: &'a FluentBundle<FluentResource>,
    message: &str,
    args: Option<&'a HashMap<&str, FluentValue>>,
) -> Option<(Cow<'a, str>, Vec<FluentError>)> {
    let msg = bundle.get_message(message)?;
    let pat = msg.value?;
    let mut errors = Vec::new();
    let value = bundle.format_pattern(&pat, args, &mut errors);

    Some((value, errors))
}

/// Internationalisation subsystem.
#[derive(Clone)]
pub struct I18n<'bundle> {
    pub locales: &'bundle [Locale],
}

impl I18n<'static> {
    /// Load locale data.
    ///
    /// Note that this function creates static references by leaking memory.
    pub fn load() -> Result<Self, LoadLocalesError> {
        let mut locale_codes = Vec::new();
        let mut resources = Vec::new();

        for entry in fs::read_dir("./locales").map_err(LoadLocalesError::FolderRead)? {
            let entry = entry.map_err(LoadLocalesError::FolderRead)?;

            if !entry.file_type().map_err(LoadLocalesError::FolderRead)?.is_file() {
                continue;
            }

            let path = entry.path();
            let locale: LanguageTag = path.file_stem()
                .expect("file on disk has no name")
                .to_str()
                .ok_or(LoadLocalesError::LocaleNameUtf8)?
                .parse()?;

            let source = fs::read_to_string(&path)
                .map_err(|err| LoadLocalesError::LocaleRead(locale.clone(), err))?;
            let resource = match FluentResource::try_new(source) {
                Ok(res) => res,
                Err((res, errors)) => {
                    error!("Errors loading locale {}:\n{}",
                        locale, format_parse_errors(&errors));

                    res
                }
            };

            locale_codes.push(locale);
            resources.push(resource);
        }

        let mut locales = Vec::new();

        for (code, resource) in locale_codes.into_iter().zip(resources.into_iter()) {
            let mut bundle: FluentBundle<FluentResource> = FluentBundle::new(&[code.as_unic().clone()]);
            bundle.add_function("JOIN", join_fun)?;

            if let Err(errors) = bundle.add_resource(resource) {
                error!("Errors loading locale {}:{}",
                    code, format_errors(&errors));
            }

            locales.push(Locale::new(code, bundle)?);
        }

        Ok(I18n {
            locales: Box::leak(locales.into_boxed_slice()),
        })
    }
}

impl<'bundle> I18n<'bundle> {
    /// Find locale by it's code.
    pub fn find_locale(&self, code: &LanguageTag)
    -> Option<&'bundle Locale> {
        self.locales.iter().find(|locale| locale.code.as_str() == code.as_str())
    }

    pub fn match_locale(&self, ranges: &[LanguageRange])
    -> &'bundle Locale {
        for range in ranges {
            for pattern in range.fallback_chain() {
                for locale in self.locales.iter() {
                    if locale.code.as_str() == pattern {
                        return locale
                    }
                }
            }
        }

        // TODO: configure default locale.
        &self.locales[0]
    }
}

#[derive(Debug, Fail, From)]
pub enum LoadLocalesError {
    #[fail(display = "can't read locale directory")]
    FolderRead(#[cause] std::io::Error),
    #[fail(display = "can't read file for locale {}", _0)]
    LocaleRead(LanguageTag, #[cause] std::io::Error),
    #[fail(display = "locale name is not valid UTF-8")]
    LocaleNameUtf8,
    #[fail(display = "locale name is not a valid language tag: {}", _0)]
    LocaleNameTag(#[cause] #[from] LanguageIdentifierError),
    #[fail(display = "Locale name missing from {}", _0)]
    MissingLocaleName(LanguageTag),
    #[fail(display = "Locale name for {} is not valid", _0)]
    InvalidLocaleName(LanguageTag),
    #[fail(display = "{}", _0)]
    Fluent(#[cause] #[from] FluentError),
}

fn format_parse_errors(errors: &[ParserError]) -> String {
    let mut result = String::new();

    for error in errors.iter() {
        let _ = write!(result, "\n    {}: {:?}", error.pos.0, error.kind);
    }

    result
}

fn format_errors(errors: &[FluentError]) -> String {
    let mut result = String::new();

    for error in errors.iter() {
        let _ = write!(result, "\n    {}", error);
    }

    result
}


fn join_fun<'a>(positional: &[FluentValue<'a>], _: &FluentArgs)
-> FluentValue<'a> {
    let r = positional.iter()
        .filter_map(|v| match v {
            FluentValue::String(ref s) | FluentValue::Number(ref s) =>
                Some(s.as_ref()),
            _ => None,
        })
        .collect();

    FluentValue::String(Cow::Owned(r))
}
