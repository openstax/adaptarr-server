# Editing process management endpoints



## Models ######################################################################

### `Process`

```
{
    id: number,
    name: string,
}
```

This model is used throughout the API to describe processes. The fields are

- `id`: process's ID;

- `name`: process's name;

### `Version`

```
{
    id: number,
    name: string,
    version: data,
}
```

This model is used throughout the API to describe versions of editing processes.
The fields are

- `id`: version's ID;

- `name`: process's name as of this version;

- `version`: this version's identifier (date and time when it was created).

### `Tree`

```
{
    name: string,
    start: number,
    slots: {
        id: number,
        name: string,
        roles: number[],
        autofill: boolean,
    }[],
    steps: {
        id: number,
        name: string,
        slots: {
            slot: number,
            permission: SlotPermission,
        }[],
        links: {
            name: string,
            to: number,
            slot: number,
        }[],
    }[],
}
```

This models is used to describe structure of an editing process. The fields are

- `name`: process's name;

- `start`: ID of the first step in this process;

- `slots`: array of slots in this editing process;

- `slots.id`: slot's ID;

- `slots.name`: slot's name;

- `slots.roles`: when not `null` and not empty, this slot is limited to only
  users who are assigned to one of the roles named by IDs in this array;

- `slots.autofill`: when true, this slot will be automatically filled with
  a user when it becomes active, assuming that a user matching slot's criteria
  (role limit) can be found;

- `steps`: array of steps in this editing process;

- `steps.id`: step's ID;

- `steps.name`: step's name;

- `steps.slots`: list of slots and slot permissions they are given at this step;

- `steps.slots.slot`: slot's ID;

- `steps.slots.permission`: permission given to `slot`;

- `steps.links`: list of possible ways in which the draft can move to another
  step;

- `steps.links.name`: link's name;

- `steps.links.to`: target step's ID;

- `steps.links.slot`: ID of the slot which can use this link.

### `NewTree`

This model is a variation of the [`Tree`](#tree) model, used to describe
a process to be created. It differs from it in following ways:

- there are no `id` fields (IDs will be assigned by the server once the process
  is created).

- `start` is an index into `steps` instead of step's ID;

- `steps.slots.slot` and `steps.links.slot` are indices into `slots` instead of
  a slot's ID;

- `steps.links.to` is an index into `steps` instead of a step's ID;

### `Slot`

```
{
    id: number,
    name: string,
    roles: number[],
}
```

Used to represent a single slot in an editing process.

- `id`: slot's ID;

- `name`: slot's name;

- `roles`: when not empty, this slot is limited to only users who are assigned
  to one of the roles named by IDs in this array.

### `Step`

```
{
    id: number,
    process: [number, number],
    name: string,
    slots: StepSlot[],
    links: Link[],
}
```

Used to represent a single step in an editing process.

- `id`: step's ID;

- `process`: IDs of process and version this step is a part of.

- `name`: step's name;

- `slots`:

- `links`: list of links originating at this step.

### `StepSlot`

```
{
    slot: number,
    permissions: SlotPermission[],
    user: number | null,
}
```

Used to represent assignment of slots to steps.

- `slot`: slot's ID;

- `permissions`: list of permissions a given slot has in a particular step;

- `user`: if this model is returned in context of a draft, this field contains
  ID of the user currently assigned to `slot` (or `null` if there isn't one).
  In all other contexts this field is `null`.

### `Link`

```
{
    to: number,
    slot: number,
    name: string,
}
```

Used to represent a single link in an editing process.

- `to`: ID of the target step;

- `slot`: ID of the slot allowed to use this link;

- `name`: link's name.



## Endpoints ###################################################################

### `GET /api/v1/processes`

Return list of all processes in the system, as a JSON array of objects of the
[`Process`](#process) model.

### `POST /api/v1/processes`

Create a new process. Accepts a JSON object of the [`NewTree`](#newtree) model.

This endpoint is only available in elevated sessions with the
[`editing-process:edit`](../#p-process-edit) permission.

#### Status codes

- 201: a new process was created. Response contains a JSON object of the
  [`Process`](#process) model.

- 400 `edit-process:new:exists`: new process could not be created because there
  already exists a process with the same name.

This endpoint can also return all errors returned from
[`POST /api/v1/processes/:id/versions`](#post-apiv1processesidversions).

### `POST /api/v1/processes/slots`

Assign current user to a slot. Accepts either
`application/x-www-form-urlencoded` or a JSON object, with following
fields/properties:

```
{
    draft: uuid,
    slot: number,
}
```

- `draft`: UUID of module in a draft of which to assign the user;

- `slot`: ID of the slot to which to assign.

#### Status codes

- 204: user was assigned.

- 400 `edit-process:slot:fill:bad-role`: user could not be assigned because they
  lack required role.

### `GET /api/v1/processes/slots/free`

Return list of all unoccupied slots which current user can take, as a JSON
array of objects of the [`Slot`](#slot) model, with following additional
properties:

```
{
    draft: Draft,
}
```

- `draft`: draft in which this slot can be taken.

### `GET /api/v1/processes/:id`

Return detailed information about a particular process, as a JSON object of the
[`Process`](#process) model.

### `PUT /api/v1/processes/:id`

Modify a process. Accepts a JSON object with following properties

```
{
    name: string,
}
```

- `name`: process's new name.

This endpoint is only available in elevated sessions with the
[`editing-process:edit`](../#p-editing-process-edit) permission.

#### Status code

- 200: process was modified. Response contains a JSON object of the
  [`Process`](#process) model, describing the process with changed applied.

### `DELETE /api/v1/processes/:id`

Delete a process. Only processes which have never been used can be deleted.

This endpoint is only available in elevated sessions with the
[`editing-process:edit`](../#p-editing-process-edit) permission.

#### Status codes

- 204: process was deleted.

### `GET /api/v1/processes/:id/slots`

Return list of all slots in the newest version of a process, as a JSON array of
objects of the [`Slot`](#slot) model.

### `GET /api/v1/processes/:id/slots/:slot`

Return detailed information about a particular slot in the newest version of
a process, as a JSON object of the [`Slot`](#slot) model.

### `PUT /api/v1/processes/:id/slots/:slot`

Modify a slot in the newest version of a process. Accepts the same body as
[`PUT /api/v1/processes/:id/versions/:version/slots/:slot`](
#put-apiv1processesidversionsversionslotsslot).

This endpoint is only available in elevated sessions with the
[`editing-process:edit`](../#p-editing-process-edit) permission.

### `GET /api/v1/processes/:id/steps`

Return list of all steps in the newest version of a process, as a JSON array of
objects of the [`Step`](#step) model.

### `GET /api/v1/processes/:id/steps/:step`

Return detailed information about a particular step in the newset version of
a process, as a JSON object of the [`Step`](#step) model.

### `PUT /api/v1/processes/:id/steps/:step`

Modify a step in the newest version of a process. Accepts the same body as
[`PUT /api/v1/processes/:id/versions/:version/steps/:step`](
#put-apiv1processesidversionsversionstepsstep).

This endpoint is only available in elevated sessions with the
[`editing-process:edit`](../#p-editing-process-edit) permission.

### `GET /api/v1/processes/:id/steps/:step/links`

Return list of all link in a particular step in the newest version of a process,
as a JSON array of objects of the [`Link`](#link) model.

### `GET /api/v1/processes/:id/steps/:step/links/:slot/:target`

Return detailed information about a particular link in a step of the newest
version of a process, as a JSON object of the [`Link`](#link) model.

### `PUT /api/v1/processes/:id/steps/:step/links/:slot/:target`

Modify a link in the newest version of a process. Accepts the same body as
[`PUT /api/v1/processes/:id/versions/:version/steps/:step/links/:slot/:target`](
#put-apiv1processesidversionsversionstepssteplinksslottarget).

This endpoint is only available in elevated sessions with the
[`editing-process:edit`](../#p-editing-process-edit) permission.

### `GET /api/v1/processes/:id/structure`

Get detailed structure of this process, as a JSON object of the [`Tree`](#tree)
model.

### `GET /api/v1/processes/:id/versions`

Return list of all versions of a process, as a JSON array of objects of the
[`Version`](#version) process.

### `POST /api/v1/processes/:id/versions`

Create a new version of an editing process. Accepts a JSON object of the
[`NewTree`](#newtree) model.

This endpoint is only available in elevated sessions with the
[`editing-process:edit`](../#p-editing-process-edit) permission.

#### Status codes

- 201: new version was created. Response contains a JSON object of the
  [`Version`](#version) model, describing the new version.

- 400 `edit-process:new:invalid-description`: new version could not be created
  because provided structure was invalid.

### `GET /api/v1/processes/:id/versions/:version`

Return detailed information about a process's version, as a JSON object of the
[`Version`](#version) model.

### `GET /api/v1/processes/:id/versions/:version/:id/slots`

Return list of all slots in a particular version of a process, as a JSON array
of objects of the [`Slot`](#slot) model.

### `GET /api/v1/processes/:id/versions/:version/:id/slots/:slot`

Return detailed information about a particular slot in a version of a process,
as a JSON object of the [`Slot`](#slot) model.

### `PUT /api/v1/processes/:id/versions/:version/:id/slots/:slot`

Modify a slot in a particular version of a process. Accepts a JSON object with
following properties:

```
{
    name: string?,
    roles: number[]?,
}
```

- `name`: slot's new name;

- `roles`: slot's new role limit.

Optional fields may be omitted, in which case the corresponding property will
remain unchanged. This endpoint is only available in elevated sessions with the
[`editing-process:edit`](../#p-editing-process-edit) permission.

### `GET /api/v1/processes/:id/versions/:version/:id/steps`

Return list of all steps in a particular version of a process, as a JSON array
of objects of the [`Step`](#step) model.

### `GET /api/v1/processes/:id/versions/:version/:id/steps/:step`

Return detailed information about a particular step in the a version of
a process, as a JSON object of the [`Step`](#step) model.

### `PUT /api/v1/processes/:id/versions/:version/:id/steps/:step`

Modify a step in a particular version of a process. Accepts a JSON object with
following properties:

```
{
    name: string,
}
```

- `name`: step's new name.

This endpoint is only available in elevated sessions with the
[`editing-process:edit`](../#p-editing-process-edit) permission.

### `GET /api/v1/processes/:id/versions/:version/:id/steps/:step/links`

Return list of all link in a particular step in a version of a process,
as a JSON array of objects of the [`Link`](#link) model.

### `GET /api/v1/processes/:id/versions/:version/:id/steps/:step/links/:slot/:target`

Return detailed information about a particular link in a step of a version of
a process, as a JSON object of the [`Link`](#link) model.

### `PUT /api/v1/processes/:id/versions/:version/:id/steps/:step/links/:slot/:target`

Modify a link in a particular version of a process. Accepts a JSON object with
following properties:

```
{
    name: string,
}
```

- `name`: link's new name.

This endpoint is only available in elevated sessions with the
[`editing-process:edit`](../#p-editing-process-edit) permission.

### `GET /api/v1/processes/:id/versions/:version/structure`

Get detailed structure of this version of an editing process, as a JSON object
of the [`Tree`](#tree) model.



## Common error codes ##########################################################

- 404 `edit-process:not-found`: returned when the `:id` specified doesn't match
  any existing editing process.

- 404 `edit-process:slot:not-found`: returned when the `:id` specified doesn't
  match any existing slot.
