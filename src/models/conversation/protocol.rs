use actix_web::ws::CloseCode;
use bitflags::bitflags;
use bytes::{BufMut, Bytes, BytesMut};
use chrono::NaiveDateTime;

#[derive(Debug)]
#[repr(transparent)]
pub struct Cookie(u32);

impl Cookie {
    /// Is this a cookie for a server-sent event?
    pub fn is_server(&self) -> bool {
        self.0 & 0x8000_0000 != 0
    }

    /// Is this a cookie for a client-sent event?
    pub fn is_client(&self) -> bool {
        self.0 & 0x8000_0000 == 0
    }
}

#[derive(Default)]
pub struct CookieGenerator(u32);

impl CookieGenerator {
    pub fn next(&mut self) -> Cookie {
        let cookie = self.0;
        self.0 += 1;

        if self.0 >= 0x8000 {
            self.0 = 0;
        }

        Cookie(cookie | 0x8000)
    }
}

#[derive(Clone, Copy)]
#[repr(u16)]
pub enum Kind {
    /// Sent to a client who just connected.
    Connected = 0,
    /// Server informs client of a new message added to the conversation.
    NewMessage = 1,
    /// Client requests to add a message to the conversation.
    SendMessage = 2,
    /// Sent as a response to an unrecognised event.
    UnknownEvent = 0x8000,
    /// Message has been successfully added to the conversation.
    MessageReceived = 0x8001,
    /// Message was not valid.
    MessageInvalid = 0x8002,
}

impl Kind {
    /// Is this message an event?
    pub fn is_event(&self) -> bool {
        (*self as u16) & 0x8000 == 0
    }

    /// Is this message a response?
    pub fn is_response(&self) -> bool {
        (*self as u16) & 0x8000 != 0
    }

    /// Get a message kind from its code.
    pub fn from_u16(v: u16) -> Option<Kind> {
        match v {
            0 => Some(Kind::Connected),
            1 => Some(Kind::NewMessage),
            2 => Some(Kind::SendMessage),
            0x8000 => Some(Kind::UnknownEvent),
            0x8001 => Some(Kind::MessageReceived),
            0x8002 => Some(Kind::MessageInvalid),
            _ => None,
        }
    }
}

bitflags! {
    pub struct Flags: u16 {
        /// This message must be processed.
        const MUST_PROCESS = 0x0001;
        /// This message requires a response.
        const RESPONSE_REQUIRED = 0x0002;
    }
}

#[derive(Debug)]
pub struct Message {
    pub cookie: Cookie,
    pub kind: u16,
    pub flags: Flags,
    pub body: Bytes,
}

impl Message {
    /// Create a message with an empty body.
    ///
    /// This function can be used in conjunction with [`Message::write`] to
    /// write just a message header into a buffer.
    pub fn header(cookie: Cookie, kind: Kind, flags: Flags) -> Self {
        Message {
            cookie,
            kind: kind as u16,
            flags,
            body: Bytes::new(),
        }
    }

    /// Build a message.
    pub fn build<B: MessageBody>(cookie: Cookie, body: B) -> BytesMut {
        let mut bytes = BytesMut::with_capacity(8 + body.length());
        Message::header(cookie, B::kind(), body.flags()).write(&mut bytes);
        body.write(&mut bytes);
        bytes
    }

    /// Parse a message header.
    pub fn parse(msg: Bytes) -> Result<Message, ParseMessageError> {
        if msg.len() < 8 {
            return Err(ParseMessageError::Underflow);
        }

        let cookie = u32::from_le_bytes([msg[0], msg[1], msg[2], msg[3]]);
        let kind = u16::from_le_bytes([msg[4], msg[5]]);
        let flags = Flags::from_bits(u16::from_le_bytes([msg[6], msg[7]]))
            .ok_or(ParseMessageError::BadFlags)?;
        let body = msg.slice_from(8);

        Ok(Message {
            cookie: Cookie(cookie),
            kind, flags, body,
        })
    }

    /// Write this message into the provided buffer.
    ///
    /// Note that to write just the message header you can simply create
    /// a `Message` object with an empty body.
    pub fn write(&self, into: &mut BytesMut) {
        into.reserve(8);
        into.put_u32_le(self.cookie.0);
        into.put_u16_le(self.kind);
        into.put_u16_le(self.flags.bits());
        into.extend_from_slice(&self.body);
    }

    /// Write this message into a new buffer.
    ///
    /// Note that to write just the message header you can simply create
    /// a `Message` object with an empty body.
    ///
    /// Note also that if the message has an empty body this method will not
    /// allocate, as the resulting buffer will fit on stack (see [`Buffer`] for
    /// details on this optimization).
    pub fn to_bytes(&self) -> BytesMut {
        let mut bytes = BytesMut::with_capacity(8 + self.body.len());
        self.write(&mut bytes);
        bytes
    }
}

#[derive(Clone, Copy)]
pub enum ParseMessageError {
    /// Message has fewer than 8 bytes.
    Underflow,
    /// Message specified unknown flags.
    BadFlags,
}

impl ParseMessageError {
    /// Get WebSocket close code to use when terminating as a result of this
    /// error.
    pub fn close_code(&self) -> CloseCode {
        match self {
            ParseMessageError::Underflow => CloseCode::Other(4000),
            ParseMessageError::BadFlags => CloseCode::Other(4004),
        }
    }
}

pub trait MessageBody {
    /// What kind of message is this?
    fn kind() -> Kind;

    /// Flags for this message.
    ///
    /// Default implementation returns an empty set.
    fn flags(&self) -> Flags {
        Flags::empty()
    }

    /// Length in bytes of message's body.
    ///
    /// The value returned from this method will be used to pre-allocate
    /// a buffer to write the message to.
    ///
    /// If the length cannot easily be determined, this function should instead
    /// return the lower bound.
    ///
    /// Default implementation returns zero.
    fn length(&self) -> usize {
        0
    }

    /// Write body of this message into a buffer.
    ///
    /// The buffer will have at least [`MessageBody::length()`] bytes allocated.
    /// Implementations are allowed to write out more data.
    fn write(self, into: &mut BytesMut);
}

/// Structure representing the body of a _0x0001 new message_ event.
pub struct NewMessage {
    /// Message's ID.
    pub id: i32,
    /// User who sent this message.
    pub user: i32,
    /// When this message was sent.
    pub timestamp: NaiveDateTime,
    /// Message body.
    pub message: Bytes,
}

impl MessageBody for NewMessage {
    fn kind() -> Kind { Kind::NewMessage }

    fn flags(&self) -> Flags { Flags::MUST_PROCESS }

    fn length(&self) -> usize {
        // header size: 2
        // message ID: 4
        // user ID: 4
        // timestamp: 8
        18
    }

    fn write(self, buf: &mut BytesMut) {
        buf.put_u16_le(self.length() as u16);
        buf.put_i32_le(self.id);
        buf.put_i32_le(self.user);
        buf.put_i64_le(self.timestamp.timestamp());
        buf.extend_from_slice(&self.message);
    }
}

/// Structure representing the body of a _0x8001 message received_ response.
pub struct MessageReceived {
    /// ID assigned to the message.
    pub id: i32,
}

impl MessageBody for MessageReceived {
    fn kind() -> Kind { Kind::MessageReceived }
    fn length(&self) -> usize { 4 }

    fn write(self, buf: &mut BytesMut) {
        buf.put_i32_le(self.id);
    }
}

/// Structure representing the body of a _0x8002 message invalid_ response.
pub struct MessageInvalid {
    pub message: Option<String>,
}

impl MessageBody for MessageInvalid {
    fn kind() -> Kind { Kind::MessageInvalid }

    fn write(self, buf: &mut BytesMut) {
        buf.extend_from_slice(
            self.message.as_ref().map_or(&[], String::as_bytes));
    }
}
