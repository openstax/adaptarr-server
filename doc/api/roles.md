# Role management endpoints



## Models ######################################################################

### `Role`

```
{
    id: number,
    name: string,
    permissions: Permission[]?,
}
```

Used throughout the API to describe roles. The fields are

- `id`: role's ID;

- `name`: role's name;

- `permissions`: role's permissions. This fields is only returned to users with
  the [`role:edit`](../#p-role-edit) permission.



## Endpoints ###################################################################

### `GET /api/v1/roles`

Return list of all roles in the system, as a JSON array of objects of the
[`Role`](#role) model.

### `POST /api/v1/roles`

Create a new role. Accepts a JSON object with following properties:

```
{
    name: string,
    permissions: Permission[],
}
```

- `name`: new role's name;

- `permissions`: set of permissions users assigned to this role will receive.

This endpoint is only available in elevated sessions with the [`role:edit`](
../#p-role-edit) permission.

#### Status codes

- 201: a role was created. Response contains a JSON object of the
  [`Role`](#role) model describing the newly created role.

- 400 `role:new:exists`: a role with specified `name` already exists.

### `GET /api/v1/roles/:id`

Return detailed information about a particular role, as a JSON object of the
[`Role`](#role) model.

### `PUT /api/v1/roles/:id`

Modify a role. Accepts a JSON object with following properties:

```
{
    name: string?,
    permissions: Permission[]?,
}
```

- `name`: role's new name;

- `permissions`: role's new permission set.

All fields may be omitted, in which case no action is taken.

This endpoint is only available in elevated sessions with the [`role:edit`](
../#p-role-edit) permission.

#### Status codes

- 200: role was updated. Response contains a JSON object of the [`Role`](#role),
  describing the role with changes applied.

### `DELETE /api/v1/roles/:id`

Delete a role.

This endpoint is only available in elevated sessions with the [`role:edit`](
../#p-role-edit) permission.

#### Status codes

- 204: role was deleted.

- 400 `role:delete:in-use`: role can't be deleted as it is still assigned to at
  lest one user.



## Common status codes #########################################################

- 404 `role:not-found`: specified `:id` doesn't match any existing role.
