use actix_web::{
    FromRequest,
    HttpRequest,
    HttpResponse,
    ResponseError,
    dev::Payload,
    http::header::{IF_MATCH, ToStrError},
};
use adaptarr_macros::From;
use failure::Fail;
use std::borrow::Cow;

/// Entity tag.
#[derive(Clone, Debug)]
pub struct EntityTag<'a> {
    strength: TagStrength,
    tag: Cow<'a, str>,
}

/// Entity tag strength.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TagStrength {
    Strong,
    Weak,
}

/// Result of comparison between two entity tags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TagEquality {
    /// Tags are equal and are both strong.
    Strong,
    /// Tags are equal, but are not both strong,
    Weak,
    /// Tags are different.
    None,
}

impl<'a> EntityTag<'a> {
    pub fn new<T>(strength: TagStrength, tag: T)
    -> Result<Self, ParseEntityTagError>
    where
        Cow<'a, str>: From<T>,
    {
        let tag: Cow<'a, str> = From::from(tag);

        if let Some(byte) = tag.as_bytes().iter().find(|&&b| {
            b != 0x21 && (b < 0x23 || b > 0x7e) && b < 0x80
        }) {
            return Err(ParseEntityTagError::BadByte(*byte));
        }

        Ok(EntityTag { strength, tag })
    }

    pub fn strong<T>(tag: T) -> Result<Self, ParseEntityTagError>
    where
        Cow<'a, str>: From<T>,
    {
        Self::new(TagStrength::Strong, tag)
    }

    pub fn weak<T>(tag: T) -> Result<Self, ParseEntityTagError>
    where
        Cow<'a, str>: From<T>,
    {
        Self::new(TagStrength::Weak, tag)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(tag: &'a str) -> Result<Self, ParseEntityTagError> {
        let (strength, tag) = if tag.starts_with("W/") {
            (TagStrength::Weak, &tag[2..])
        } else {
            (TagStrength::Strong, tag)
        };

        if !tag.starts_with('"') || !tag.ends_with('"') {
            return Err(ParseEntityTagError::NotQuoted);
        }

        let tag = &tag[1..tag.len() - 1];

        Self::new(strength, tag)
    }

    pub fn to_owned(&self) -> EntityTag<'static> {
        let tag = match self.tag {
            Cow::Borrowed(b) => Cow::Owned(b.to_owned()),
            Cow::Owned(ref b) => Cow::Owned(b.clone()),
        };

        EntityTag {
            strength: self.strength,
            tag,
        }
    }

    pub fn compare(&self, other: &EntityTag) -> TagEquality {
        match (self.strength, other.strength) {
            (TagStrength::Strong, TagStrength::Strong)
            if self.tag == other.tag => TagEquality::Strong,
            _ if self.tag == other.tag => TagEquality::Weak,
            _ => TagEquality::None,
        }
    }
}

#[derive(Debug, Fail, From)]
pub enum ParseEntityTagError {
    #[fail(display = "entity tag is not quoted")]
    NotQuoted,
    #[fail(display = "entity tag contains invalid byte {}", _0)]
    BadByte(u8),
    #[fail(display = "{}", _0)]
    String(#[from] ToStrError),
}

impl ResponseError for ParseEntityTagError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::BadRequest().body(self.to_string())
    }
}

/// The If-Match header allows specifying an expected version(s) of a resource,
/// and can be used for example to prevent lost update errors.
pub enum IfMatch<'a> {
    /// Any version of the resource will match.
    Any,
    /// Only specified versions of the resource will match.
    OneOf(Vec<EntityTag<'a>>),
}

impl<'a> IfMatch<'a> {
    pub fn is_any(&self) -> bool {
        match self {
            IfMatch::Any => true,
            _ => false,
        }
    }

    /// Test this header against a known entity tag. Returns true if it matches
    /// (there has been no change to the resource) and false if it doesn't
    /// (there has been a change to the resource).
    pub fn test(&self, tag: &EntityTag) -> bool {
        match self {
            IfMatch::Any => true,
            IfMatch::OneOf(tags) =>
                tags.iter().any(|t| t.compare(tag) == TagEquality::Strong),
        }
    }
}

impl FromRequest for IfMatch<'static> {
    type Config = ();
    type Error = ParseEntityTagError;
    type Future = Result<IfMatch<'static>, Self::Error>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let header = match req.headers().get(IF_MATCH) {
            Some(header) => header,
            None => return Ok(IfMatch::Any),
        };

        if header == "*" {
            return Ok(IfMatch::Any);
        }

    let tags = header.to_str()?.split(',')
            .map(str::trim)
            .map(EntityTag::from_str)
            .map(|r| r.map(|tag| tag.to_owned()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(IfMatch::OneOf(tags))
    }
}
