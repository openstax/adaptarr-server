use failure::Fail;
use serde::{de::{Deserialize, Deserializer, Error}, ser::{Serialize, Serializer}};
use std::{fmt, str::FromStr};
use unic_langid::{LanguageIdentifier, errors::LanguageIdentifierError};

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct LanguageTag(String, LanguageIdentifier);

impl LanguageTag {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn as_unic(&self) -> &LanguageIdentifier {
        &self.1
    }
}

impl fmt::Display for LanguageTag {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for LanguageTag {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        <&str as Deserialize>::deserialize(de)?
            .parse()
            .map_err(D::Error::custom)
    }
}

impl Serialize for LanguageTag {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&self.0)
    }
}

impl FromStr for LanguageTag {
    type Err = LanguageIdentifierError;

    fn from_str(v: &str) -> Result<LanguageTag, Self::Err> {
        let id = v.parse()?;
        Ok(LanguageTag(v.to_string(), id))
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct LanguageRange(String);

impl LanguageRange {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Generate fall-back locale chain for the locale lookup algorithm,
    /// as defined in [RFC 4647, §3.4](
    /// https://tools.ietf.org/html/rfc4647#section-3.4).
    pub fn fallback_chain(&self) -> impl Iterator<Item = &str> {
        FallbackChain { tag: &self.0 }
    }
}

impl fmt::Display for LanguageRange {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for LanguageRange {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        <&str as Deserialize>::deserialize(de)?
            .parse()
            .map_err(D::Error::custom)
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
            if inx > 2 && self.tag[inx - 2..].starts_with('-') {
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

impl FromStr for LanguageRange {
    type Err = ParseLanguageTagError;

    fn from_str(v: &str) -> Result<LanguageRange, Self::Err> {
        let mut chars = v.char_indices();

        'outer: loop {
            match chars.next() {
                None =>
                    return Err(ParseLanguageTagError::ExpectedSubtag(v.len())),
                Some((_, '0'..='9')) | Some((_, 'a'..='z')) |
                Some((_, 'A'..='Z')) => loop {
                    match chars.next() {
                        Some((_, '0'..='9')) | Some((_, 'a'..='z')) |
                        Some((_, 'A'..='Z')) => {}
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
