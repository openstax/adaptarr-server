# Events and notifications



## Models ######################################################################

### `Event`

```
{
    id: number,
    kind: string,
    timestamp: date,
}
```

Event's data consist of general data and type-specific data. General data
contains those fields:

- `id`: event's ID;

- `kind`: event's type;

- `timestamp`: date and time when this event occurred.

Event types and their type-specific data is described in
[Types of events](#types-of-events).



## Endpoints ###################################################################

### `GET /api/v1/notifications`

Get list of all unread notifications received by current user, as a JSON array
of objects of the [`Event`](#event) model.

### `PUT /api/v1/notifications/:id`

Update a notification's state. Accents a JSON object with following properties:

```
{
    unread: boolean,
}
```

- `unread`: when true marks this notification as read.

#### Status codes

- 204: notification's state was updated.

### `GET /events`

Open a WebSocket connection. Each time an event is emitted for the current user,
a JSON object of the [`Event`](#event) model will be send on this connection.



## Common status codes #########################################################

- 404 `event:not-found`: specified `:id` doesn't match any existing event owned
  by current user.



## Types of events #############################################################

### `assigned`

> *NOTE:* This event is now obsolete.

Emitted when a user was assigned to a module. Event data contains ID of the user
who assigned (`who`) and of the module (`module`).

```js
{
    who: number,
    module: UUID,
}
```

### `process-ended`

Emitted when editing process in which use participated has been concluded. Event
data contains ID of the module for which the process has ended.

```js
{
    module: UUID,
}
````

### `slot-filled` and `slot-vacated`

Emitted when user is assigned to (`slot-filled`) or removed from
(`slot-vacated`) a slot in an editing process. Event data contains ID of
the slot (`slot`) and the module (`module`).

```js
{
    slot: number,
    module: UUID,
}
````

### `draft-advanced`

Emitted when a draft moves between editing steps. Event data contains ID of
the module (`module`), ID of the step into which it has moved (`step`), and
a list of editing permissions the user now possesses (`permissions`).

```js
{
    module: UUID,
    step: number,
    permissions: string[],
}
````
