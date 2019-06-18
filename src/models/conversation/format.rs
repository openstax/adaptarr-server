use bitflags::bitflags;
use failure::Fail;
use std::{io::Read, str::Utf8Error};

/// Known frame types.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u64)]
pub enum Frame {
    Message = 0,
    Paragraph = 1,
    Text = 2,
    PushFormat = 3,
    PopFormat = 4,
    Hyperlink = 5,
    Mention = 6,
}

static LINE_CONTEXT: &[Frame] = &[
    Frame::Text, Frame::PushFormat, Frame::PopFormat, Frame::Hyperlink,
    Frame::Mention,
];

static BLOCK_CONTEXT: &[Frame] = &[Frame::Paragraph];

impl Frame {
    fn from_u64(n: u64) -> Option<Frame> {
        match n {
            0 => Some(Frame::Message),
            1 => Some(Frame::Paragraph),
            2 => Some(Frame::Text),
            3 => Some(Frame::PushFormat),
            4 => Some(Frame::PopFormat),
            5 => Some(Frame::Hyperlink),
            6 => Some(Frame::Mention),
            _ => None,
        }
    }

    /// Get a list of frames this frames can contain.
    ///
    /// This list is sorted.
    fn can_contain(&self) -> &'static [Frame] {
        match *self {
            Frame::Message => BLOCK_CONTEXT,
            Frame::Paragraph => LINE_CONTEXT,
            Frame::Text => &[],
            Frame::PushFormat => &[],
            Frame::PopFormat => &[],
            Frame::Hyperlink => &[],
            Frame::Mention => &[],
        }
    }
}

bitflags! {
    /// Known formatting flags.
    struct Format: u16 {
        const EMPHASIS = 0x0001;
        const STRONG = 0x0002;
    }
}

/// Result of message validation.
#[derive(Default)]
pub struct Validation<'a> {
    /// List of users mentioned in this message.
    pub mentions: Vec<i32>,
    /// Portion of the input data containing the message.
    pub body: &'a [u8],
    /// Remaining bytes not interpreted as part of the message.
    pub rest: &'a [u8],
}

#[derive(Debug, Fail)]
pub enum ValidationError {
    #[fail(display = "{}", _0)]
    Io(#[cause] std::io::Error),
    #[fail(display = "message contains a LEB128 value greater than 2^64 - 1")]
    Leb128Overflow,
    #[fail(
        display = "frame {:?} declares length {} greater than message length {}",
        _0, _1, _2,
    )]
    FrameOverflow(Frame, usize, usize),
    #[fail(display = "expected frame type {:?} to have {} bytes, but found {}",
        _0, _1, _2)]
    FrameLength(Frame, usize, usize),
    #[fail(display = "message contains unknown frame {}", _0)]
    UnknownFrame(u64),
    #[fail(display = "{:?} is not a valid root frame", _0)]
    BadRoot(Frame),
    #[fail(display = "frame {:?} cannot contain frame {:?}", _0, _1)]
    BadChild(Frame, Frame),
    #[fail(display = "{}", _0)]
    Text(#[cause] Utf8Error),
    #[fail(display = "message contains unknown formatting {}", _0)]
    UnknownFormat(u16),
    #[fail(display = "message contains a non-ASCII URL")]
    NonAsciiUrl,
}

impl_from! { for ValidationError ;
    std::io::Error => |e| ValidationError::Io(e),
    Utf8Error => |e| ValidationError::Text(e),
}

/// Read a single LEB128 value from a reader.
fn leb128<R: Read>(mut r: R) -> Result<u64, ValidationError> {
    let mut buf = [0; 1];
    let mut v = 0u64;

    loop {
        r.read_exact(&mut buf)?;
        v = v.checked_shl(7)
            .and_then(|v| v.checked_add(u64::from(buf[0] & 0x7f)))
            .ok_or(ValidationError::Leb128Overflow)?;

        if buf[0] & 0x80 == 0 {
            break
        }
    }

    Ok(v)
}

/// Read a stream of frames.
fn frames(mut bytes: &[u8])
-> impl Iterator<Item = Result<(Frame, &[u8]), ValidationError>> {
    std::iter::from_fn(move || {
        if bytes.is_empty() {
            None
        } else {
            Some(read_frame(&mut bytes))
        }
    })
}

/// Read a single frame.
fn read_frame<'a>(bytes: &mut &'a [u8])
-> Result<(Frame, &'a [u8]), ValidationError> {
    let ty = leb128(&mut *bytes)?;
    let ty = Frame::from_u64(ty).ok_or(ValidationError::UnknownFrame(ty))?;
    let size = leb128(&mut *bytes)? as usize;

    if size > bytes.len() {
        Err(ValidationError::FrameOverflow(ty, size, bytes.len()))
    } else {
        let (body, rest) = bytes.split_at(size);
        *bytes = rest;
        Ok((ty, body))
    }
}

/// Validate contents of a user-sent message.
pub fn validate(message: &[u8]) -> Result<Validation, ValidationError> {
    let mut read = message;
    let mut ctx = Validation::default();
    let (ty, body) = read_frame(&mut read)?;

    if ty != Frame::Message {
        return Err(ValidationError::BadRoot(ty));
    }

    validate_frame(&mut ctx, ty, body)?;

    let len = read.as_ptr() as usize - message.as_ptr() as usize;
    ctx.body = &message[..len];
    ctx.rest = read;
    Ok(ctx)
}

/// Validate a single complex frame.
fn validate_frame(ctx: &mut Validation, ty: Frame, body: &[u8])
-> Result<(), ValidationError> {
    let legal = ty.can_contain();

    for frame in frames(body) {
        let (frame, body) = frame?;

        if legal.binary_search(&frame).is_err() {
            return Err(ValidationError::BadChild(ty, frame));
        }

        match frame {
            Frame::Message =>
                unreachable!("There are no frames that can contain Message"),
            Frame::Paragraph => validate_frame(ctx, frame, body)?,
            Frame::Text => validate_text(body)?,
            Frame::PushFormat | Frame::PopFormat => validate_format(frame, body)?,
            Frame::Hyperlink => validate_hyperlink(body)?,
            Frame::Mention => validate_mention(ctx, body)?,
        }
    }

    Ok(())
}

/// Validate a text ([`Frame::Text`]) frame.
fn validate_text(body: &[u8]) -> Result<(), ValidationError> {
    std::str::from_utf8(body)?;
    Ok(())
}

/// Validate a formatting ([`Frame::PushFormat`] or [`Frame::PopFormat`]) frame.
fn validate_format(frame: Frame, body: &[u8]) -> Result<(), ValidationError> {
    if body.len() != 2 {
        return Err(ValidationError::FrameLength(frame, 2, body.len()));
    }

    let bits = u16::from_le_bytes([body[0], body[1]]);

    Format::from_bits(bits)
        .ok_or(ValidationError::UnknownFormat(bits & !Format::all().bits()))?;

    Ok(())
}

/// Validate a hyperlink ([`Frame::Hyperlink`]) frame.
fn validate_hyperlink(mut body: &[u8]) -> Result<(), ValidationError> {
    let len = leb128(&mut body)?;
    let (label, url) = body.split_at(len as usize);
    validate_text(label)?;

    let url = std::str::from_utf8(url)?;
    if !url.is_ascii() {
        Err(ValidationError::NonAsciiUrl)
    } else {
        Ok(())
    }
}

/// Validate a mention ([`Frame::Mention`]) frame.
fn validate_mention(ctx: &mut Validation, mut body: &[u8])
-> Result<(), ValidationError> {
    let user = leb128(&mut body)? as i32;

    ctx.mentions.push(user);

    Ok(())
}
