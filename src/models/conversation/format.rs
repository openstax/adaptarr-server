use adaptarr_macros::From;
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
    pub struct Format: u16 {
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

#[derive(Debug, Fail, From)]
pub enum Error {
    #[fail(display = "{}", _0)]
    Io(#[cause] #[from] std::io::Error),
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
    Text(#[cause] #[from] Utf8Error),
    #[fail(display = "message contains unknown formatting {}", _0)]
    UnknownFormat(u16),
    #[fail(display = "message contains a non-ASCII URL")]
    NonAsciiUrl,
}

/// Read a single LEB128 value from a reader.
fn leb128<R: Read>(mut r: R) -> Result<u64, Error> {
    let mut buf = [0; 1];
    let mut v = 0u64;

    loop {
        r.read_exact(&mut buf)?;
        v = v.checked_shl(7)
            .and_then(|v| v.checked_add(u64::from(buf[0] & 0x7f)))
            .ok_or(Error::Leb128Overflow)?;

        if buf[0] & 0x80 == 0 {
            break
        }
    }

    Ok(v)
}

/// Read a stream of frames.
fn frames(mut bytes: &[u8])
-> impl Iterator<Item = Result<(Frame, &[u8]), Error>> {
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
-> Result<(Frame, &'a [u8]), Error> {
    let ty = leb128(&mut *bytes)?;
    let ty = Frame::from_u64(ty).ok_or(Error::UnknownFrame(ty))?;
    let size = leb128(&mut *bytes)? as usize;

    if size > bytes.len() {
        Err(Error::FrameOverflow(ty, size, bytes.len()))
    } else {
        let (body, rest) = bytes.split_at(size);
        *bytes = rest;
        Ok((ty, body))
    }
}

/// Validate contents of a user-sent message.
pub fn validate(message: &[u8]) -> Result<Validation, Error> {
    let mut read = message;
    let mut ctx = Validation::default();
    let (ty, body) = read_frame(&mut read)?;

    if ty != Frame::Message {
        return Err(Error::BadRoot(ty));
    }

    validate_frame(&mut ctx, ty, body)?;

    let len = read.as_ptr() as usize - message.as_ptr() as usize;
    ctx.body = &message[..len];
    ctx.rest = read;
    Ok(ctx)
}

/// Validate a single complex frame.
fn validate_frame(ctx: &mut Validation, ty: Frame, body: &[u8])
-> Result<(), Error> {
    let legal = ty.can_contain();

    for frame in frames(body) {
        let (frame, body) = frame?;

        if legal.binary_search(&frame).is_err() {
            return Err(Error::BadChild(ty, frame));
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
fn validate_text(body: &[u8]) -> Result<(), Error> {
    std::str::from_utf8(body)?;
    Ok(())
}

/// Validate a formatting ([`Frame::PushFormat`] or [`Frame::PopFormat`]) frame.
fn validate_format(frame: Frame, body: &[u8]) -> Result<(), Error> {
    if body.len() != 2 {
        return Err(Error::FrameLength(frame, 2, body.len()));
    }

    let bits = u16::from_le_bytes([body[0], body[1]]);

    Format::from_bits(bits)
        .ok_or(Error::UnknownFormat(bits & !Format::all().bits()))?;

    Ok(())
}

/// Validate a hyperlink ([`Frame::Hyperlink`]) frame.
fn validate_hyperlink(mut body: &[u8]) -> Result<(), Error> {
    let len = leb128(&mut body)?;
    let (label, url) = body.split_at(len as usize);
    validate_text(label)?;

    let url = std::str::from_utf8(url)?;
    if !url.is_ascii() {
        Err(Error::NonAsciiUrl)
    } else {
        Ok(())
    }
}

/// Validate a mention ([`Frame::Mention`]) frame.
fn validate_mention(ctx: &mut Validation, mut body: &[u8])
-> Result<(), Error> {
    let user = leb128(&mut body)? as i32;

    ctx.mentions.push(user);

    Ok(())
}

/// Read contents of a message.
pub fn reader(mut message: &[u8]) -> Result<FrameReader, Error> {
    let (frame, body) = read_frame(&mut message)?;

    if frame != Frame::Message {
        return Err(Error::BadRoot(frame));
    }

    Ok(FrameReader { frame, body })
}

pub struct FrameReader<'a> {
    pub frame: Frame,
    body: &'a [u8],
}

impl<'a> FrameReader<'a> {
    pub fn iter(self) -> impl Iterator<Item = Result<FrameReader<'a>, Error>> {
        let legal = self.frame.can_contain();

        frames(self.body).map(move |frame| {
            let (frame, body) = frame?;

            if legal.binary_search(&frame).is_err() {
                return Err(Error::BadChild(self.frame, frame));
            }

            Ok(FrameReader { frame, body })
        })
    }
}

/// Render a message to a custom renderer.
pub fn render<R>(message: &[u8], mut renderer: R) -> Result<R::Result, Error>
where
    R: Renderer,
{
    for frame in reader(message)?.iter() {
        let frame = frame?;

        match frame.frame {
            Frame::Paragraph => {
                renderer.begin_paragraph();

                let mut format = Format::empty();

                for frame in frame.iter() {
                    let frame = frame?;

                    match frame.frame {
                        Frame::Text => {
                            renderer.text(read_text(frame.body)?);
                        }
                        Frame::PushFormat => {
                            let flags = read_format(frame.frame, frame.body)?;
                            if !format.contains(flags) {
                                format.insert(flags);
                                renderer.push_format(flags, format);
                            }
                        }
                        Frame::PopFormat => {
                            let flags = format & read_format(
                                frame.frame, frame.body)?;
                            if !flags.is_empty() {
                                format.remove(flags);
                                renderer.pop_format(flags, format);
                            }
                        }
                        Frame::Hyperlink => {
                            let (label, url) = read_hyperlink(frame.body)?;
                            renderer.hyperlink(label, url);
                        }
                        Frame::Mention => {
                            let user = read_mention(frame.body)?;
                            renderer.mention(user);
                        }
                        _ => unreachable!(),
                    }
                }

                renderer.end_paragraph();
            }
            _ => unreachable!(),
        }
    }

    Ok(renderer.finish())
}

/// Message renderer.
pub trait Renderer {
    /// Result of rendering a message.
    type Result;

    /// Begin rendering a paragraph.
    fn begin_paragraph(&mut self);

    /// Stop rendering a paragraph.
    fn end_paragraph(&mut self);

    /// Add a text fragment.
    fn text(&mut self, text: &str);

    /// Apply specified formatting to subsequent text.
    ///
    /// First argument contains the formatting to apply, second the effective
    /// cumulative formatting.
    fn push_format(&mut self, format: Format, current: Format);

    /// Stop applying specified formatting to subsequent text.
    ///
    /// First argument contains the formatting to stop applying, second the
    /// effective cumulative formatting.
    fn pop_format(&mut self, format: Format, current: Format);

    /// Add a hyperlink.
    fn hyperlink(&mut self, label: Option<&str>, url: &str);

    /// Add a user mention.
    fn mention(&mut self, user: i32);

    /// Finalize rendering and produce final result.
    fn finish(self) -> Self::Result;
}

/// Read a text ([`Frame::Text`]) frame.
fn read_text(body: &[u8]) -> Result<&str, Error> {
    std::str::from_utf8(body).map_err(From::from)
}

/// Read a formatting ([`Frame::PushFormat`] or [`Frame::PopFormat`]) frame.
fn read_format(frame: Frame, body: &[u8]) -> Result<Format, Error> {
    if body.len() != 2 {
        return Err(Error::FrameLength(frame, 2, body.len()));
    }

    let bits = u16::from_le_bytes([body[0], body[1]]);

    Format::from_bits(bits)
        .ok_or(Error::UnknownFormat(bits & !Format::all().bits()))
}

/// Read a hyperlink ([`Frame::Hyperlink`]) frame.
fn read_hyperlink(mut body: &[u8]) -> Result<(Option<&str>, &str), Error> {
    let len = leb128(&mut body)?;
    let (label, url) = body.split_at(len as usize);

    let label = if label.is_empty() {
        None
    } else {
        Some(read_text(label)?)
    };

    let url = std::str::from_utf8(url)?;
    if !url.is_ascii() {
        Err(Error::NonAsciiUrl)
    } else {
        Ok((label, url))
    }
}

/// Read a mention ([`Frame::Mention`]) frame.
fn read_mention(mut body: &[u8]) -> Result<i32, Error> {
    leb128(&mut body).map(|x| x as i32)
}
