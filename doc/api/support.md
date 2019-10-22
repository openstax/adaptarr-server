# Technical support



## Models ######################################################################

### `Ticket`

```
{
    id: number,
    title: string,
    opened: date,
    authors: number[],
    conversation: Conversation,
}
```

Representation of a _support ticket_; a user's request for help. Fields are

- `id`: ticket's ID;

- `title`: ticket's title;

- `opened`: date and time when this ticked was opened;

- `authors`: list of IDs of this ticket's authors;

- `conversation`: instance of the [`Conversation`](
  ../../conversation.md#Conversation) model for the conversation between
  the user and the support team.



## Endpoints ###################################################################

### `GET /api/v1/support/tickets`

Return list of all tickets, as a JSON array of objects of the [`Ticket`](
#ticket) model. Users who are not part of the support team will only receive
tickets they opened.

### `POST /api/v1/support/tickets`

Open a new ticket. Accepts either `application/x-www-form-urlencoded` or a JSON
object with following fields/properties:

```
{
    title: string,
}
```

- `title`: ticket's title.

#### Status codes

- 201: a new ticket was created. Response contains a JSON object of the
  [`Ticket`](#ticket) model, describing the new ticket.

### `GET /api/v1/support/tickets/my`

Return list of all tickets opened by current user, as a JSON array of objects of
the [`Ticket`](#ticket) model.

Unlike [`GET /api/v1/support/tickets`](#get-apiv1supporttickets) this endpoint
returns only tickets opened by current user, regardless of whether or not they
are a member of the support team.

### `GET /api/v1/support/tickets/:id`

Return detailed information about a particular draft, as a JSON object of the
[`Ticket`](#ticket) model.

### `PUT /api/v1/support/tickets/:id`

Modify a ticket. Accepts either `application/x-www-form-urlencoded` or a JSON
object with following fields/properties:

```
{
    title: string?,
}
```

- `title`: ticket's new title;

This endpoint can only be used by members of the support team.

### `POST /api/v1/support/tickets/:id/join`

Join the conversation associated with this ticket. This endpoint is only
available to members of the support team.
