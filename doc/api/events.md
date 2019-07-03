### Types of events

#### `assigned`

> *NOTE:* This event is now obsolete.

Emitted when a user was assigned to a module. Event data contains ID of the user
who assigned (`who`) and of the module (`module`).

```js
{
    who: number,
    module: UUID,
}
```

#### `process-ended`

Emitted when editing process in which use participated has been concluded. Event
data contains ID of the module for which the process has ended.

```js
{
    module: UUID,
}
````

#### `slot-filled` and `slot-vacated`

Emitted when user is assigned to (`slot-filled`) or removed from
(`slot-vacated`) a slot in an editing process. Event data contains ID of
the slot (`slot`) and the module (`module`).

```js
{
    slot: number,
    module: UUID,
}
````

#### `draft-advanced`

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
