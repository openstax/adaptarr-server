# Team management endpoints



## Models ######################################################################

### `Team`

```
{
    id: number,
    name: string,
    roles: Role[],
}
```

Used throughout the API to describe teams. The fields are

- `id`: team's ID;

- `name`: team's name;

- `roles`: list of all roles in this team.

### `TeamMember`

```
{
    user: number,
    permissions: TeamPermission[],
    role: Role | null,
}
```

Used throughout the API to describe members of a team. The fields are

- `user`: ID of a [`User`](../users.md#User) who is a member of a team;

- `permissions`: list of team permissions `user` has;

- `role`: [`Role`](#role) held by `user` in the team.

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
  the [`role:edit`](../#p-role-edit) permission in the team.



## Endpoints ###################################################################

### `GET /api/v1/teams`

Return list of all teams current user is a member of, as a JSON array of objects
of the [`Team`](#team) model.

In elevated sessions a list of all team in the system is returned instead.

### `POST /api/v1/teams`

Create a new team. Accepts either `application/x-www-form-urlencoded` or a JSON
object with following fields/properties:

```
{
    name: string,
}
```

- `name`: team's name.

This endpoint is only available in elevated sessions with the [`team:manage`](
../#p-team-manage) permission.

### `GET /api/v1/teams/:id`

Return detailed information about a particular team, as a JSON object of the
[`Team`](#team) model.

### `PUT /api/v1/teams/:id`

Modify a team. Accepts either `application/x-www-form-urlencoded` or a JSON
object with following fields/properties:

```
{
    name: string,
}
```

- `name`: team's name.

This endpoint is only available in elevated sessions with the [`team:manage`](
../#p-team-manage) permission.

### `GET /api/v1/teams/:id/roles`

Return list of all roles in a system, as a JSON array of objects of the
[`Role`](#role) model.

### `POST /api/v1/teams/:id/roles`

Create a new role. Accepts a JSON object with following properties:

```
{
    name: string,
    permissions: Permission[],
}
```

- `name`: new role's name;

- `permissions`: set of permissions users assigned to this role will receive.

This endpoint is only available to users with the [`role:edit`](../#p-role-edit)
permission in the team.

#### Status codes

- 201: a role was created. Response contains a JSON object of the
  [`Role`](#role) model describing the newly created role.

- 400 `role:new:exists`: a role with specified `name` already exists.

### `GET /api/v1/teams/:id/roles/:role`

Return detailed information about a particular role, as a JSON object of the
[`Role`](#role) model.

### `PUT /api/v1/teams/:id/roles/:role`

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

This endpoint is only available to users with the [`role:edit`](../#p-role-edit)
permission in the team.

#### Status codes

- 200: role was updated. Response contains a JSON object of the [`Role`](#role),
  describing the role with changes applied.

### `DELETE /api/v1/teams/:id/roles/:role`

Delete a role.

This endpoint is only available to users with the [`role:edit`](../#p-role-edit)
permission in the team.

#### Status codes

- 204: role was deleted.

- 400 `role:delete:in-use`: role can't be deleted as it is still assigned to at
  lest one user.

### `GET /api/v1/teams/:id/members`

Get list of all members of a team as a JSON list of objects of the
[`TeamMember`](#teammember) model.

### `POST /api/v1/teams/:id/members`

Add a new member to a team. Accepts either `application/x-www-form-urlencoded`
or a JSON object with following fields/properties:

```
{
    user: number | string,
    permissions: TeamPermission[],
    role: number?,
}
```

- `user`: when this field is a `number` it specifies ID of an existing user,
  when it is a string it specifies an email address of a new user to invite to
  the platform.

- `permissions`: list of permissions `user` will held in this team. Must be
  a subset of permissions held by current user.

- `role`: ID of a role to assign the new member to. When `null`, `user` will not
  be assigned a role.

This endpoint is only available to users with the [`member:add`](
../#p-member-add) permission in the team.

#### Status codes

- 202: invitation has been accepted for processing. Sending invitation may still
  fail because the email might not exist.

### `GET /api/v1/teams/:id/members/:member`

Return detailed information about a particular member, as a JSON object of the
[`TeamMember`](#teammember) model.

### `PUT /api/v1/teams/:id/members/:member`

Modify a team member. Accepts either `application/x-www-form-urlencoded`  or
a JSON object with following fields/properties:

```
{
    permissions: TeamPermission[]?,
    role: (number | null)?,
}
```

- `permissions`: a set of permissions to assign to the user. Any permissions
  which user already has but which are not present in this field will be
  removed. Permissions which the current user doesn't have will be unaffected.

  This field can only be used by a user with the [`member:edit-permissions`](
  ../#p-member-edit-permissions) permission in this team.

- `role`: when this field is a number it is an ID of a role to which the member
  will be assigned, when it is `null` the member will instead be unassigned from
  any role.

  This field can only be used by a user with the [`member:assign-role`](
  ../#p-member-assign-role) permission in this team.

### `DELETE /api/v1/teams/:id/members/:member`

Remove a user from this team.

This endpoint is only available to users with the [`member:remove`](
../#p-member-remove) permission in the team.



## Common status codes #########################################################

- 404 `role:not-found`: specified `:id` doesn't match any existing role.
