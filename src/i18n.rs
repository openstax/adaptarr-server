use fluent::{FluentBundle, FluentResource};
use fluent_bundle::{errors::FluentError, types::FluentValue};
use fluent_syntax::parser::errors::ParserError;
use serde::{de, ser};
use std::{
    collections::HashMap,
    fmt::{self, Write as _},
    fs,
    marker::PhantomData,
    str::FromStr,
};

use crate::Result;

/// Internationalisation subsystem.
#[derive(Clone)]
pub struct I18n<'bundle> {
    resources: &'bundle [FluentResource],
    pub locales: &'bundle [Locale<'bundle>],
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct LanguageTag(String);

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct LanguageRange(String);

#[derive(Serialize)]
pub struct Locale<'bundle> {
    pub code: LanguageTag,
    pub name: String,
    #[serde(skip_serializing)]
    messages: FluentBundle<'bundle>,
}

impl I18n<'static> {
    /// Load locale data.
    ///
    /// Note that this function creates static references by leaking memory.
    pub fn load() -> Result<Self> {
        let mut locale_codes = Vec::new();
        let mut resources = Vec::new();

        for entry in fs::read_dir("./locales")? {
            let entry = entry?;

            if !entry.file_type()?.is_file() {
                continue;
            }

            let path = entry.path();
            let locale = path.file_stem()
                .expect("file on disk has no name")
                .to_str()
                .ok_or(I18nError::LocaleNameUtf8)?
                .parse()?;

            let source = fs::read_to_string(&path)?;
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

        let resources = Box::leak(resources.into_boxed_slice());
        let mut locales = Vec::new();

        for (code, resource) in locale_codes.into_iter().zip(resources.iter()) {
            let mut bundle = FluentBundle::new(&[&code]);

            if let Err(errors) = bundle.add_resource(&resource) {
                error!("Errors loading locale {}:{}",
                    code, format_errors(&errors));
            }

            locales.push(Locale::new(code, bundle)?);
        }

        Ok(I18n {
            resources: resources,
            locales: Box::leak(locales.into_boxed_slice()),
        })
    }
}

impl<'bundle> I18n<'bundle> {
    /// Find locale by it's code.
    pub fn find_locale(&self, code: &LanguageTag)
    -> Option<&'bundle Locale<'bundle>> {
        self.locales.iter().find(|locale| locale.code.0 == code.0)
    }

    pub fn match_locale(&self, ranges: &[LanguageRange])
    -> &'bundle Locale<'bundle> {
        for range in ranges {
            for pattern in range.fallback_chain() {
                for locale in self.locales.iter() {
                    if locale.code.0 == pattern {
                        return locale
                    }
                }
            }
        }

        // TODO: configure default locale.
        &self.locales[0]
    }
}

#[derive(Debug, Fail)]
pub enum I18nError {
    #[fail(display = "Locale name is not valid UTF-8")]
    LocaleNameUtf8,
    #[fail(display = "Locale name missing from {}", _0)]
    MissingLocaleName(LanguageTag),
    #[fail(display = "Locale name for {} is not valid", _0)]
    InvalidLocaleName(LanguageTag),
}

impl<'bundle> Locale<'bundle> {
    fn new(code: LanguageTag, messages: FluentBundle<'bundle>) -> Result<Self> {
        let (name, errors) = match messages.format("locale-name", None) {
            Some(v) => v,
            None => return Err(I18nError::MissingLocaleName(code).into()),
        };

        if errors.is_empty() {
            Ok(Locale { code, name, messages })
        } else {
            error!("Could not format message locale-name in locale {}", code);
            Err(I18nError::InvalidLocaleName(code).into())
        }
    }

    pub fn format(&self, key: &str, args: &HashMap<&str, FluentValue>)
    -> Option<String> {
        let (value, errors) = self.messages.format(key, Some(args))?;

        if errors.is_empty() {
            Some(value)
        } else {
            error!("Could not format message {} in locale {}:{}",
                key, self.code, format_errors(errors.as_slice()));
            None
        }
    }
}

impl LanguageTag {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for LanguageTag {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(&self.0)
    }
}

impl LanguageRange {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Generate fall-back locale chain for the locale lookup algorithm,
    /// as defined in [RFC 4647, §3.4](
    /// https://tools.ietf.org/html/rfc4647#section-3.4).
    fn fallback_chain(&self) -> impl Iterator<Item = &str> {
        FallbackChain { tag: &self.0 }
    }
}

impl fmt::Display for LanguageRange {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(&self.0)
    }
}

#[derive(Debug)]
struct FallbackChain<'tag> {
    tag: &'tag str,
}

impl<'tag> Iterator for FallbackChain<'tag> {
    type Item = &'tag str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.tag.is_empty() {
            return None;
        }

        let value = self.tag;

        if let Some(inx) = self.tag.rfind('-') {
            if inx > 2 && self.tag[inx - 2..].starts_with("-") {
                self.tag = &self.tag[..inx-2];
            } else {
                self.tag = &self.tag[..inx];
            }
        } else {
            self.tag = "";
        }

        Some(value)
    }
}

impl FromStr for LanguageTag {
    type Err = ParseLanguageTagError;

    fn from_str(v: &str) -> Result<LanguageTag, Self::Err> {
        let mut chars = v.char_indices();

        'outer: loop {
            match chars.next() {
                None =>
                    return Err(ParseLanguageTagError::ExpectedSubtag(v.len())),
                Some((_, '0'...'9')) | Some((_, 'a'...'z')) |
                Some((_, 'A'...'Z')) => loop {
                    match chars.next() {
                        Some((_, '0'...'9')) | Some((_, 'a'...'z')) |
                        Some((_, 'A'...'Z')) => {}
                        Some((_, '-')) => continue 'outer,
                        Some((inx, _)) => return Err(
                            ParseLanguageTagError::ExpectedAlphanum(inx)),
                        None => break 'outer,
                    }
                },
                Some((inx, _)) =>
                    return Err(ParseLanguageTagError::ExpectedSubtag(inx)),
            }
        }

        Ok(LanguageTag(v.to_string()))
    }
}

impl FromStr for LanguageRange {
    type Err = ParseLanguageTagError;

    fn from_str(v: &str) -> Result<LanguageRange, Self::Err> {
        let mut chars = v.char_indices();

        'outer: loop {
            match chars.next() {
                None =>
                    return Err(ParseLanguageTagError::ExpectedSubtag(v.len())),
                Some((_, '0'...'9')) | Some((_, 'a'...'z')) |
                Some((_, 'A'...'Z')) => loop {
                    match chars.next() {
                        Some((_, '0'...'9')) | Some((_, 'a'...'z')) |
                        Some((_, 'A'...'Z')) => {}
                        Some((_, '-')) => continue 'outer,
                        Some((inx, _)) => return Err(
                            ParseLanguageTagError::ExpectedAlphanum(inx)),
                        None => break 'outer,
                    }
                },
                Some((_, '*')) => match chars.next() {
                    Some((_, '-')) => continue 'outer,
                    Some((inx, _)) => return Err(
                        ParseLanguageTagError::ExpectedSeparator(inx)),
                    None => break 'outer,
                },
                Some((inx, _)) =>
                    return Err(ParseLanguageTagError::ExpectedSubtag(inx)),
            }
        }

        Ok(LanguageRange(v.to_string()))
    }
}

#[derive(Clone, Copy, Debug, Eq, Fail, PartialEq)]
pub enum ParseLanguageTagError {
    #[fail(display = "{}: expected subtag", _0)]
    ExpectedSubtag(usize),
    #[fail(display = "{}: expected letter or digit", _0)]
    ExpectedAlphanum(usize),
    #[fail(display = "{}: expected subtag separator", _0)]
    ExpectedSeparator(usize),
}

impl<'de> de::Deserialize<'de> for LanguageTag {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        de.deserialize_str(FromStrVisitor("a language range", PhantomData))
    }
}

impl<'de> de::Deserialize<'de> for LanguageRange {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        de.deserialize_str(FromStrVisitor("a language range", PhantomData))
    }
}

struct FromStrVisitor<'a, T>(&'a str, PhantomData<T>);

impl<'a, 'de, T> de::Visitor<'de> for FromStrVisitor<'a, T>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    type Value = T;

    fn expecting(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.write_str(self.0)
    }

    fn visit_str<E>(self, v: &str) -> Result<T, E>
    where
        E: serde::de::Error,
    {
        v.parse().map_err(|err| E::custom(err))
    }
}

impl ser::Serialize for LanguageTag {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        ser.serialize_str(&self.0)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_language_tag() {
        assert_eq!("de-DE".parse(), Ok(LanguageTag("de-DE".to_string())));
        // We don't do full validation
        assert_eq!("12-45".parse(), Ok(LanguageTag("12-45".to_string())));
        assert_eq!(
            "".parse::<LanguageTag>(),
            Err(ParseLanguageTagError::ExpectedSubtag(0)),
        );
        assert_eq!(
            "de-*-DE".parse::<LanguageTag>(),
            Err(ParseLanguageTagError::ExpectedSubtag(3)),
        );
        assert_eq!(
            "de--DE".parse::<LanguageTag>(),
            Err(ParseLanguageTagError::ExpectedSubtag(3)),
        );
    }

    #[test]
    fn fallback_chain() {
        // Taken from RFC 4647, §3.4.
        let tag = LanguageRange("zh-Hant-CN-x-private1-private2".to_string());
        let chain = tag
            .fallback_chain()
            .collect::<Vec<_>>();
        assert_eq!(chain, [
            "zh-Hant-CN-x-private1-private2",
            "zh-Hant-CN-x-private1",
            "zh-Hant-CN",
            "zh-Hant",
            "zh",
        ]);
    }
}
