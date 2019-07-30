# Adaptarr! live conversation protocol



## Protocol ####################################################################

Applications using this protocol communicate by asynchronously exchanging
binary messages over a WebSockets connection.

There are two types of messages: events and responses. Events are sent
asynchronously by either side to notify the other, or to request an action
from the other. Responses are special types of messages emitted in response to
certain events.

Whether an event requires a response is mandated by message flags of that event.
Applications can send responses to events which don't require one, and shall
ignore responses they were not expecting. At most one response can be sent for
each event.

Messages consist of an 8-byte header and a variable length body. The entire
message must be transmitted within a single WebSocket message. The first four
bytes are the message cookie, followed by two byte message type. The last two
bytes are message flags.

Cookies are used to identify which to event a message is a response.
Applications are free to chose any value as a cookie as long as it satisfies
following conditions:

1.  Cookies for server events must have highest bit set.
2.  Cookies for client events must have highest bit cleared.
3.  Application should not re-use cookies if possible.
4.  Application must not re-use a cookie if it is still expecting reply to
    an event previously using that cookie.
5.  When it is not possible not to re-use a cookie (for example because the
    application sent over 2 147 483 647 messages), application should try to
    re-use the oldest cookie.

Message type is used to distinguish different types of messages from each other.
They also define how body of a message is to be interpreted. While events and
responses can be distinguished by cookies alone they should nonetheless have
distinct types. By convention, events should have types below 0x8000 and
responses above or equal to 0x8000 (or alternatively, responses should have
highest bit of type set).

Message flags is a bit-field used to customise handling of an event. There are
currently no flags defined for responses. Currently defined flags are::

- Bit 1: Message must be processed.

  If this bit is set and an application doesn't understand the message (it has
  an unknown type), it must terminate the connection with error 4001. If this
  bit is unset the application is free to ignore the message. In such case it
  must send response 0x8000.

- Bit 2: Response required.

  The application must send a response to this event. The application must
  process this event and generate a response before generating responses to
  events received after this one. The sender shall terminate the connection with
  error code 4002 if it timed out waiting for a response, or with code 4003 if
  it received a response for a later event before a response for this event.

If a message has set flags not specified here, the application must not process
it and must terminate connection with error 4004.



### Messages

#### 0x0000 Connected

Send by the server to a client who just connected. Contains basic information
about the conversation. This message should not be send by a client.

TODO: decide what exactly should be included in this message,

#### 0x0001 New message

Sent by the server to inform the client of a new message. The body contains
metadata and a single [conversation message](#conversation-message).

The metadata starts with a 2-byte length, allowing new fields to be added to the
metadata. Fields will never be removed from the metadata. Thus the length also
serves as a version indicator. Length also includes itself, and can be used as
an offset to the start of message. Current format has 18 bytes and contains:

- The length field (2 bytes).
- message's ID (4 bytes)
- ID of the user who authored this message (4 bytes).
- Timestamp of when this message was sent (8 bytes, signed), encoded as a number
  of seconds since the UNIX epoch.

#### 0x0002 Send message

Sent by the client to add a new message to the conversation. The body contains
a single [conversation message](#conversation-messages).

#### 0x0003 Get history

Sent by the client to request a fragment of conversation's history. Body
specifies requested range of entries as the ID of newest known event (4 byte
integer) and number of events (2 byte integer). Node that server may return
fewer events than requested (e.g. because of rate limiting, or just because
there aren't as many events). Zero can be used instead of a reference event's ID
to request newest events.



### Responses

#### 0x8000 Unknown event

Application received an unknown event but was not required to process it. This
message has no body.

#### 0x8001 Message received

Sent in response to 0x0001 to indicate that the message has been successfully
processed and recorded in a conversation. Message body is a single 4-byte number
containing the ID assigned to the new message.

#### 0x8002 Message invalid

Sent in response to 0x0001 if the message failed validation. Message body is
a UTF-8 encoded string describing the problem. This is only intended as an aid
in debugging a client application and may not be included in a response sent by
a production server.

#### 0x8003 History entries

Send in response to 0x0003. History is returned as a list of significant
messages (that is messages which affect the conversation), such as the client
would have received had it been connected when they were first emitted.

The message body contains number of messages (2 byte integer) followed by
history entries. Each entry begins with message's type (2 byte integer, the same
as described in this document), it's length ([LEB128]), and then entry's body as
it would be sent in a standalone message. There are no gaps or padding between
entries.

Messages included in history as of this version are: [0x0001 New message](
#0x0001-new-message).



### Connection termination

Connections are terminated by closing the WebSocket they are using. If connection
is being terminated as a result of an error condition, that condition shall be
indicated using WebSocket's custom error codes (error codes in range 4000
to 4999). Currently defined error codes are:

- 4000: Application received an invalid message (this usually means that the
  message had an incomplete header).
- 4001: Application received an unknown event and was required to process it.
- 4002: Application timed out waiting for a mandatory response to an event.
- 4003: Application did not receive a mandatory response to an event, but
  received response to a later event.
- 4004: Application received a message with an unknown flag set.



## Conversation messages #######################################################

Conversation messages are stored and transmitted in a simple binary format. This
format was developed to be easy to verify, easy to process, small, and easy to
extend, in that order.

The main building block of a message is a _frame_. Frames are typed,
variable-length containers. Each frame begins with two [LEB128]-encoded numbers
indicating its type and length in bytes of its body, followed by its body. Most
frames will contain only a small piece of data. Those are called _simple frames_.
_Complex frames_ are frames which contain other frames (called _subframes_). The
entire message itself is a complex frame type 0.

Frame type numbers are globally unique; no two different frames may share the
same type number, even if they can never be used in the same context.

[LEB128]: https://en.wikipedia.org/wiki/LEB128



### Message structure

Structure of messages is intentionally kept very simple.

There are three ways (or contexts) in which frames can be used to build
messages: line, block, and text-block.

<a name="line-context"></a>_Line context_ frames, called such because they build
lines of text, are the most basic building blocks of a message. Those frames
define text, formatting, and dynamic text-like inserts (such as mentions) that
comprise the bulk of a message. They are all simple frames.

Line context and text formatting is described in more detail in the
[next section](#line-context-section).

<a name="block-context"></a>_Block context_ frames are the exact opposite. They
define the structure (where elements are located in relation to one another) of
a message, but are otherwise invisible. They are all complex frames.

Currently [message](#message) and [paragraph](#paragraph) are the only block
frames.

<a name="text-block-context"></a>_Text-block context_ frames connect those two
classes together. They are block frames (that is frames which are children of
block context frames), but contain only line context frames. All

Currently [paragraph](#paragraph) is the only text-block frame.

In short: _block context_ defines the structure of a message, _text-block
context_ defines where in that structure text may appear, and _line context_
fills those places with text.



### <a name="line-context-section"></a> Line context

Text is encoded as a sequence of line elements. The most basic of those is
[text](#text). This element defines a simple, unformatted fragment of text.
There should never be adjacent text frames, but renderers and processors must
accept such construction.

Aside from just simple text, line context may also contain _inline elements_.
Those frames describe additional non-textual content which is often intermixed
with text, such as hyperlinks or user mentions.

Text formatting can be controlled via formatting flags. When rendering a text
fragment the renderer will apply to it styles defined by current value of the
<a name="formatting-flags"></a>_formatting flags_. Those flags in turn are
controlled using the [push formatting](#push-formatting) and [pop formatting](
#pop-formatting) frames. Unlike other line frames, those frames do not render
anything by themselves, rather they affect value of formatting flags. Both
frames contain just a set of style flags, which the renderer will either add to
(push formatting) or remove from (pop formatting) its formatting flags.
Currently defined formatting flags are:

- Bit 1: Emphasis, often rendered in italics.
- Bit 2: Strong, often rendered in bold.

Currently defined line elements are [text](#text), [push formatting](
#push-formatting), [pop formatting](#pop-formatting), [hyperlink](#hyperlink),
 and [mention](#mention).



### Frames

#### 0 Message

This is the root frame of the message. Wrapping message in this frame makes it
possible to know message's length up front, and thus to, for example, transport
multiple messages in a single byte stream.

Message may only contain [block context frames](#block-context).

#### 1 Paragraph

A single paragraph of text. This is the most basic text-block element.

Paragraphs may only contain [line context frames](#line-context).

#### 2 Text

A single, unbroken, UTF-8 encoded fragment of text. This is the most basic line
element.

#### 3 Push formatting

Add values to current [formatting flags](#formatting-flags). Contains a single
two-byte unsigned integer specifying which flags to enable.

#### 4 Pop formatting

Remove values to current [formatting flags](#formatting-flags). Contains
a single two-byte unsigned integer specifying which flags to disable.

#### 5 Hyperlink

A hyperlink. Contains link label as UTF-8 text preceded by LEB128 length, and
followed by ASCII URL. Label must not begin or end with white space (as defined
by Unicode). If label is empty (its length is zero) the URL will be used as
label.

Hyperlink should be rendered as its label, formatted such that the user can
easily identify it as a hyperlink. Clicking the hyperlink should open a new
browser tab and redirect it to the URL.

#### 6 Mention

Mention of an user. The server will notify the user that they were mentioned.
Additionally, if the mentioned user did not already have access to the
conversation they will be granted it, starting with this message (that is they
will not be able to access messages prior to the first one mentioning them).

The body of this frame is a single LEB128 number containing user's ID.
