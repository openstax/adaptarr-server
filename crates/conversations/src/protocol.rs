use actix_web_actors::ws::CloseCode;
use adaptarr_macros::From;
use adaptarr_util::{BufExt, BufMutExt};
use bitflags::bitflags;
use bytes::{Buf, BufMut, Bytes, BytesMut, IntoBuf};
use chrono::{DateTime, Utc, TimeZone};
use failure::Fail;

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
    /// Client requests a slice of conversation's history.
    GetHistory = 3,
    /// Sent as a response to an unrecognised event.
    UnknownEvent = 0x8000,
    /// Message has been successfully added to the conversation.
    MessageReceived = 0x8001,
    /// Message was not valid.
    MessageInvalid = 0x8002,
    /// History entries are being returned.
    HistoryEntries = 0x8003,
}

impl Kind {
    /// Is this message an event?
    pub fn is_event(self) -> bool {
        (self as u16) & 0x8000 == 0
    }

    /// Is this message a response?
    pub fn is_response(self) -> bool {
        (self as u16) & 0x8000 != 0
    }

    /// Get a message kind from its code.
    pub fn from_u16(v: u16) -> Option<Kind> {
        match v {
            0 => Some(Kind::Connected),
            1 => Some(Kind::NewMessage),
            2 => Some(Kind::SendMessage),
            3 => Some(Kind::GetHistory),
            0x8000 => Some(Kind::UnknownEvent),
            0x8001 => Some(Kind::MessageReceived),
            0x8002 => Some(Kind::MessageInvalid),
            0x8003 => Some(Kind::HistoryEntries),
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
    pub fn parse(msg: Bytes) -> Result<Message, ParseHeaderError> {
        if msg.len() < 8 {
            return Err(ParseHeaderError::Underflow);
        }

        let cookie = u32::from_le_bytes([msg[0], msg[1], msg[2], msg[3]]);
        let kind = u16::from_le_bytes([msg[4], msg[5]]);
        let flags = Flags::from_bits(u16::from_le_bytes([msg[6], msg[7]]))
            .ok_or(ParseHeaderError::BadFlags)?;
        let body = msg.slice_from(8);

        Ok(Message {
            cookie: Cookie(cookie),
            kind, flags, body,
        })
    }

    /// Parse body of this message.
    pub fn parse_body<B: MessageBody>(&self) -> Result<B, ParseMessageError> {
        B::read(self.body.clone())
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
pub enum ParseHeaderError {
    /// Message has fewer than 8 bytes.
    Underflow,
    /// Message specified unknown flags.
    BadFlags,
}

impl ParseHeaderError {
    /// Get WebSocket close code to use when terminating as a result of this
    /// error.
    pub fn close_code(self) -> CloseCode {
        match self {
            ParseHeaderError::Underflow => CloseCode::Other(4000),
            ParseHeaderError::BadFlags => CloseCode::Other(4004),
        }
    }
}

pub enum ParseMessageError {
    /// Message body was too short.
    Underflow(/* expected at least */ usize, /* got */ usize),
    /// Message contained nested data of an unknown kind.
    UnknownKind(u16),
    /// Message body was invalid.
    Other(Box<dyn Fail>),
}

impl<E: Fail> From<E> for ParseMessageError {
    fn from(e: E) -> Self {
        ParseMessageError::Other(Box::new(e))
    }
}

pub trait MessageBody: Sized {
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

    /// Read body of this message from a buffer.
    fn read(from: Bytes) -> Result<Self, ParseMessageError>;
}

#[derive(From)]
pub enum AnyMessage {
    Connected(#[from] Connected),
    NewMessage(#[from] NewMessage),
    SendMessage(#[from] SendMessage),
    GetHistory(#[from] GetHistory),
    UnknownEvent,
    MessageReceived(#[from] MessageReceived),
    MessageInvalid(#[from] MessageInvalid),
    HistoryEntries(#[from] HistoryEntries),
}

impl AnyMessage {
    pub fn kind(&self) -> Kind {
        match *self {
            AnyMessage::Connected(_) => Kind::Connected,
            AnyMessage::NewMessage(_) => Kind::NewMessage,
            AnyMessage::SendMessage(_) => Kind::SendMessage,
            AnyMessage::GetHistory(_) => Kind::GetHistory,
            AnyMessage::UnknownEvent => Kind::UnknownEvent,
            AnyMessage::MessageReceived(_) => Kind::MessageReceived,
            AnyMessage::MessageInvalid(_) => Kind::MessageInvalid,
            AnyMessage::HistoryEntries(_) => Kind::HistoryEntries,
        }
    }

    pub fn write(self, into: &mut BytesMut) {
        match self {
            AnyMessage::Connected(msg) => msg.write(into),
            AnyMessage::NewMessage(msg) => msg.write(into),
            AnyMessage::SendMessage(msg) => msg.write(into),
            AnyMessage::GetHistory(msg) => msg.write(into),
            AnyMessage::UnknownEvent => UnknownEvent.write(into),
            AnyMessage::MessageReceived(msg) => msg.write(into),
            AnyMessage::MessageInvalid(msg) => msg.write(into),
            AnyMessage::HistoryEntries(msg) => msg.write(into),
        }
    }
}

/// First event sent from the server to the client when connection is
/// established.
pub struct Connected {
}

impl MessageBody for Connected {
    fn kind() -> Kind { Kind::Connected }
    fn write(self, _: &mut BytesMut) {}
    fn read(_: Bytes) -> Result<Self, ParseMessageError> { Ok(Connected {}) }
}

/// Structure representing the body of a _0x0001 new message_ event.
pub struct NewMessage {
    /// Message's ID.
    pub id: i32,
    /// User who sent this message.
    pub user: i32,
    /// When this message was sent.
    pub timestamp: DateTime<Utc>,
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

    fn read(from: Bytes) -> Result<Self, ParseMessageError> {
        let mut buf = (&from).into_buf();
        let length = buf.get_u16_le() as usize;

        if length < 18 {
            return Err(ParseMessageError::Underflow(18, length));
        }

        let id = buf.get_i32_le();
        let user = buf.get_i32_le();
        let timestamp = Utc.timestamp(buf.get_i64_le(), 0);

        Ok(NewMessage {
            id, user, timestamp,
            message: from.slice_from(length),
        })
    }
}

/// Sent by the client to add a new message to the conversation.
pub struct SendMessage {
    pub message: Bytes,
}

impl MessageBody for SendMessage {
    fn kind() -> Kind { Kind::SendMessage }
    fn flags(&self) -> Flags { Flags::MUST_PROCESS | Flags::RESPONSE_REQUIRED }
    fn length(&self) -> usize { self.message.len() }

    fn write(self, into: &mut BytesMut) {
        into.copy_from_slice(&self.message);
    }

    fn read(from: Bytes) -> Result<Self, ParseMessageError> {
        Ok(SendMessage { message: from })
    }
}

/// Send by client to request a slice of conversation's history.
pub struct GetHistory {
    /// Reference event's ID.
    pub from: Option<i32>,
    /// Number of events preceding reference to retrieve.
    pub number_before: u16,
    /// Number of events succeeding reference to retrieve.
    pub number_after: u16,
}

impl MessageBody for GetHistory {
    fn kind() -> Kind { Kind::GetHistory }
    fn flags(&self) -> Flags { Flags::RESPONSE_REQUIRED }
    fn length(&self) -> usize { 6 }

    fn write(self, into: &mut BytesMut) {
        into.put_i32_le(self.from.unwrap_or(0));
        into.put_u16_le(self.number_before);
        into.put_u16_le(self.number_after);
    }

    fn read(from: Bytes) -> Result<Self, ParseMessageError> {
        let mut buf = from.into_buf();

        let from = buf.get_i32_le();
        let number_before = buf.get_u16_le();
        let number_after = buf.get_u16_le();

        Ok(GetHistory {
            from: if from == 0 { None } else { Some(from) },
            number_before,
            number_after,
        })
    }
}

/// Send in a response to a unrecognised event which didn't need to be
/// processed.
pub struct UnknownEvent;

impl MessageBody for UnknownEvent {
    fn kind() -> Kind { Kind::UnknownEvent }
    fn write(self, _: &mut BytesMut) {}
    fn read(_: Bytes) -> Result<Self, ParseMessageError> { Ok(UnknownEvent) }
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

    fn read(from: Bytes) -> Result<Self, ParseMessageError> {
        Ok(MessageReceived {
            id: from.into_buf().get_i32_le(),
        })
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

    fn read(from: Bytes) -> Result<Self, ParseMessageError> {
        let message = if from.is_empty() {
            None
        } else {
            Some(String::from_utf8(from.as_ref().to_vec())?)
        };

        Ok(MessageInvalid { message })
    }
}

/// Structure representing body of a _0x8003 history entries_ response.
pub struct HistoryEntries {
    pub before: Vec<AnyMessage>,
    pub after: Vec<AnyMessage>,
}

impl MessageBody for HistoryEntries {
    fn kind() -> Kind { Kind::HistoryEntries }

    fn length(&self) -> usize {
        // 2 bytes for number of entries + 2 bytes for each entry's type.
        4 + 2 * self.before.len() + 2 * self.after.len()
    }

    fn write(self, into: &mut BytesMut) {
        into.put_u16_le(self.before.len() as u16);
        into.put_u16_le(self.after.len() as u16);

        let mut buf = BytesMut::new();

        for entry in self.before.into_iter().chain(self.after) {
            let kind = entry.kind();

            buf.clear();
            entry.write(&mut buf);

            let lebsize = ((buf.len() as f64).log2() / 7f64).ceil() as usize;
            into.reserve(2 + lebsize + buf.len());
            into.put_u16_le(kind as u16);
            into.put_leb128(buf.len() as u64);
            into.extend_from_slice(&buf);
        }
    }

    fn read(from: Bytes) -> Result<Self, ParseMessageError> {
        let mut entries = Vec::new();
        let mut buf = (&from).into_buf();

        let count_before = buf.get_u16_le();
        let count_after = buf.get_u16_le();

        for _ in 0..(count_before + count_after) {
            let kind = buf.get_u16_le();
            let kind = Kind::from_u16(kind)
                .ok_or(ParseMessageError::UnknownKind(kind))?;
            let length = buf.get_leb128();

            let start = buf.position();
            let end = start + length;
            let body = from.slice(start as usize, end as usize);
            buf.set_position(end);

            entries.push(match kind {
                Kind::NewMessage => NewMessage::read(body)?.into(),
                _ => return Err(ParseMessageError::UnknownKind(kind as u16)),
            });
        }

        Ok(HistoryEntries {
            after: entries.split_off(count_before as usize),
            before: entries,
        })
    }
}
