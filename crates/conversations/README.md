# Real time conversations over WebSockets

## Adding new message types

Adding new message types is relatively easy, but it does involve some amount of
coding. The most important part is of course designing the message; what data
will it carry and how will it be encoded. This should be documented in
[conversation.md](../../../doc/conversation.md). Then to actually implement the
message:

1.  Add type definitions to [protocol.rs](./src/protocol.rs), for:

    a.  message's type ID, as a variant of `Kind`;

    b.  message's body, as a new structure implementing `MessageBody`;

    c.  add the message as a possible variant of `AnyMessage`;

2.  If the message is to be stored in database, implement loading it in
    `serialize_events()` in [client.rs](./src/client.rs).

3.  If you want to dispatch this message to connected users, add it as a variant
    of `Event` in [broker.rs](./src/broker.rs).
