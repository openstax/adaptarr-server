use adaptarr_macros::From;
use bitflags::bitflags;
use bytes::{Buf, Bytes};
use failure::Fail;
use std::str::Utf8Error;

use super::util::{BufExt, ReadBytes};

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
pub struct Validation {
    /// List of users mentioned in this message.
    pub mentions: Vec<i32>,
    /// Portion of the input data containing the message.
    pub body: Bytes,
    /// Remaining bytes not interpreted as part of the message.
    pub rest: Bytes,
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
    #[fail(display = "frame {:?} contains {} extra bytes", _0, _1)]
    FrameTooLong(Frame, usize),
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

/// Read a stream of frames.
fn frames<'b>(mut bytes: ReadBytes<'b>)
-> impl Iterator<Item = Result<(Frame, ReadBytes<'b>), Error>> + 'b {
    std::iter::from_fn(move || {
        if bytes.is_empty() {
            None
        } else {
            Some(read_frame(&mut bytes))
        }
    })
}

/// Read a single frame.
fn read_frame<'b>(body: &mut ReadBytes<'b>)
-> Result<(Frame, ReadBytes<'b>), Error> {
    let ty = body.get_leb128();
    let ty = Frame::from_u64(ty).ok_or(Error::UnknownFrame(ty))?;
    let size = body.get_leb128() as usize;

    if size > body.remaining() {
        Err(Error::FrameOverflow(ty, size, body.remaining()))
    } else {
        Ok((ty, body.slice(size)))
    }
}

/// Validate contents of a user-sent message.
pub fn validate(message: &Bytes) -> Result<Validation, Error> {
    let mut read = ReadBytes::new(message);
    let mut ctx = Validation::default();
    let (ty, body) = read_frame(&mut read)?;

    if ty != Frame::Message {
        return Err(Error::BadRoot(ty));
    }

    validate_frame(&mut ctx, ty, body)?;

    ctx.body = message.slice_to(read.cursor());
    ctx.rest = read.as_bytes();
    Ok(ctx)
}

/// Validate a single complex frame.
fn validate_frame(ctx: &mut Validation, ty: Frame, body: ReadBytes)
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
            Frame::Paragraph => { validate_frame(ctx, frame, body)?; }
            Frame::Text => { read_text(body)?; }
            Frame::PushFormat | Frame::PopFormat => { read_format(frame, body)?; }
            Frame::Hyperlink => { read_hyperlink(body)?; }
            Frame::Mention => { ctx.mentions.push(read_user_mention(body)?); }
        }
    }

    Ok(())
}

/// Read a text ([`Frame::Text`]) frame.
fn read_text(body: ReadBytes) -> Result<&str, Error> {
    std::str::from_utf8(body.as_slice()).map_err(From::from)
}

/// Read a formatting ([`Frame::PushFormat`] or [`Frame::PopFormat`]) frame.
fn read_format(frame: Frame, mut body: ReadBytes) -> Result<Format, Error> {
    if body.remaining() != 2 {
        return Err(Error::FrameLength(frame, 2, body.remaining()));
    }

    let bits = body.get_u16_le();

    Format::from_bits(bits)
        .ok_or(Error::UnknownFormat(bits & !Format::all().bits()))
}

/// Read a hyperlink ([`Frame::Hyperlink`]) frame.
fn read_hyperlink(mut body: ReadBytes) -> Result<(Option<&str>, &str), Error> {
    let len = body.get_leb128() as usize;

    let label = if len == 0 {
        None
    } else {
        Some(read_text(body.slice(len))?)
    };

    let url = std::str::from_utf8(body.as_slice())?;
    if !url.is_ascii() {
        Err(Error::NonAsciiUrl)
    } else {
        Ok((label, url))
    }
}

/// Read a mention ([`Frame::Mention`]) frame.
fn read_user_mention(mut body: ReadBytes)
-> Result<i32, Error> {
    let user = body.get_leb128() as i32;

    if !body.is_empty() {
        Err(Error::FrameTooLong(Frame::Mention, body.remaining()))
    } else {
        Ok(user)
    }
}

/// Read contents of a message.
pub fn reader(message: &Bytes) -> Result<FrameReader, Error> {
    let (frame, body) = read_frame(&mut ReadBytes::new(message))?;

    if frame != Frame::Message {
        return Err(Error::BadRoot(frame));
    }

    Ok(FrameReader { frame, body })
}

pub struct FrameReader<'b> {
    pub frame: Frame,
    body: ReadBytes<'b>,
}

impl<'b> FrameReader<'b> {
    pub fn iter(self) -> impl Iterator<Item = Result<FrameReader<'b>, Error>> {
        let FrameReader { frame, body } = self;
        let legal = frame.can_contain();

        frames(body).map(move |frame| {
            let (frame, body) = frame?;

            if legal.binary_search(&frame).is_err() {
                return Err(Error::BadChild(frame, frame));
            }

            Ok(FrameReader { frame, body })
        })
    }
}

/// Render a message to a custom renderer.
pub fn render<R>(message: &Bytes, mut renderer: R) -> Result<R::Result, Error>
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
                            let user = read_user_mention(frame.body)?;
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
