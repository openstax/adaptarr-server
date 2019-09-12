# Draft management endpoints



## Models ######################################################################

### `Draft`

```
{
    module: uuid,
    team: number,
    title: string,
    language: string,
    permissions: SlotPermission[]?,
    step: Step?,
    books: uuid[]?,
}
```

THis model is used throughout the API to describe drafts. The fields are

- `module`: UUID of the module from this draft was derived;

- `team`: ID of the team owning the module this draft was derived from;

- `title`: draft's title;

- `language`: draft's language;

- `permissions`: list of slot permissions current user has in this draft. This
  field is only present if current user occupies a slot in this draft.

- `step`: editing process step this draft is currently at;

- `books`: list of UUID of books containing the module this draft was derived
  from.

Fields `step` and `books` may be omitted. Such case are list in endpoint
documentation.



## Endpoints ###################################################################

### `GET /api/v1/drafts`

Return list of all drafts current user has session to, as a JSON array of
objects of the [`Draft`](#draft) model.

### `GET /api/v1/drafts/:id`

Return detailed information about a particular draft, as a JSON object of the
[`Draft`](#draft) model.

### `PUT /api/v1/drafts/:id`

Modify a draft. Accepts a JSON object with following properties:

```
{
    title: string,
}
```

- `title`: draft's new title.

This endpoint is only available to users with the [`edit`][#edit] slot
permission.

#### Status codes

- 200: draft was modified. Response contains a JSON object of the
  [`Draft`](#draft) model, describing the draft with changes applied.

### `DELETE /api/v1/drafts/:id`

Cancel process for a draft, discarding changes.

This endpoint is only available to users with the [`process:manage`](
../#p-process-manage) permission in the team owning the draft.

#### Status codes

- 204: draft was deleted.

### `POST /api/v1/drafts/:id/advance`

Advance a draft to a next editing step. Accepts either
`application/x-www-form-urlencoded` or a JSON object with following
fields/properties:

```
{
    target: i32,
    slot: i32,
}
```

- `target`: ID of the target step;

- `slot`: slot used to advance.

`target` and `slot` together name the link which will be used to advance the
draft.

#### Status codes

- 200: draft was advanced. Returns a JSON object with following properties

  ```
  {
    code: 'draft:process:advanced' | 'draft:process:finished',
    draft: Draft,
    module: Module,
  }
  ```

  - `code`: `'draft:process:advanced'` if the draft was advanced to the next
    step, or `'draft:process:finished'` if as a result the process has finished;

  - `draft`: description of the draft after it was advanced. Only present when
    `code` is `'draft:process:advanced'`.

  - `module`: description of the module after the process was finished and
    changes merged into it. Only present when `code` is
    `'draft:process:finished'`.

- 400 `draft:advance:bad-link`: none of the links from the current step matched
  specified `target` and `slot`.

- 400 `draft:advance:bad-slot`: `slot` specified doesn't exist, or has no
  permissions in current step.

- 403 `draft:advance:bad-user`: the user making the request doesn't occupy the
  `slot` they are trying to use.

### `GET /api/v1/drafts/:id/files`

Get list of files in this draft, excluding `index.cnxml`, as a JSON array of
strings containing names of files.

### `GET /api/v1/drafts/:id/files/:name`

Get contents of a particular file in a draft.

#### Status codes

- 404 `file:not-found`: no file with such name could be found in this module.

### `PUT /api/v1/drafts/:id/files/:name`

Update contents of a particular file in a draft, or create a new file.

This endpoint is only available to users with the [`edit`] slot permission.
Writing to `index.cnxml` is also possible with the [`accept-changes`] and
[`propose-changes`] slot permissions.

#### Status codes

- 204: file was updated or created.

### `DELETE /api/v1/drafts/:id/files/:name`

Delete a file from a draft. `index.cnxml` cannot be deleted.

This endpoint is only available to users with the [`edit`] slot permission.

#### Status codes

- 204: file was deleted.

### `GET /api/v1/drafts/:id/books`

Get list of books containing the module this draft was derived from, as a JSON
array of UUIDs of books.

### `GET /api/v1/drafts/:id/process`

Return detailed information about status of the editing process for a particular
draft. Returns a JSON object of the [`Process`] with following additional
properties:

```
{
    slots: {
        slot: Slot,
        user: User?,
    }[],
}
```

- `slots`: list of slots and users assigned to them;

- `slots.slot`: details of a slot;

- `slots.user`: details of user assigned to this slot, or `null` if no one is.

This endpoint is only available to users with the [`process:manage`](
../#p-process-manage) permission in the team owning the draft.

### `PUT /api/v1/drafts/:id/process/slots/:slot`

Assign a user to a slot. Accepts a JSON number.

This endpoint is only available to users with the [`process:manage`](
../#p-process-manage) permission in the team owning the draft.

#### Status codes

- 204: user was assigned.



## Common error codes ##########################################################

- 403 `draft:process:insufficient-permission`: action could not be completed
  because current user lacks required slot permission.

- 404 `draft:not-found`: the `:id` specified did not match any existing draft.
