# User management endpoints



## Models ######################################################################

### `User`

```
{
    id: number,
    name: string,
    is_super: boolean,
    language: string,
    permissions: Permission[]?,
    teams: [
        {
            id: number,
            role: Role | null,
        },
    ],
}
```

This model is used throughout the API to describe users. The fields are

- `id`: user's ID number;

- `name`: users' name;

- `language`: user's preferred language. Server guarantees that it is supported;

- `permissions`: list of system permissions this user has.

  This field is only returned to users with the [`user:edit-permissions`](
  ../#p-user-edit-permissions) permission, and in a few specific cases listed in
  endpoint documentation.

- `teams`: list of teams this user is member of. Only includes the teams which
  the requesting user is a member of;

- `teams.id`: team's ID;

- `teams.role`: an instance of the [`Role`](../roles.md#Role) model describing
  role assigned to the user in this team. May be `null` if the user is not
  assigned a role.

### `:id`

Many user management endpoints allow specifying which user they operate on. This
parameter is always part of endpoint's path and is denoted `:id`, and takes two
forms: a number or the string `me`. The first form selects a user by their ID,
the second select the user making the request (based on HTTP session).

Note that while the user making request can be specified in both forms, they are
considered strictly different, and some actions might be limited to only one of
them. Such exceptions are listed in endpoint documentation.



## Endpoints ###################################################################

### `GET /api/v1/users`

Return list of all users in teams current user is a member of, as a JSON array
of object of the [`User`](#user) model.

In elevated sessions a list of all users in the system is returned instead.

### `POST /api/v1/users/invite`

Create a new invitation. Accepts either `application/x-www-form-urlencoded` or
a JSON object, with following fields/properties:

```
{
    email: string,
    language: string,
    role: number?,
    team: number,
    permissions: TeamPermission[],
}
```

- `email`: email to which to send the invitation. The user will only be able to
  register using this email;

- `language`: a [BCP 47][BCP47] language tag naming the language in which to
  send the invitation. Must be one of languages supported by the server. The
  user will be able to choose a different language during registration;

- `role`: if present contains ID of a role to which the user will be assigned
  upon registration.

- `team`: ID of the team to which to invite the users. Current user must have
  the [`member:add`](../#p-member-add) permission in this team. Additionally if
  the user being invited doesn't yet have an account, current user must
  additionally have the [`user:invite`](../#p-user-invite) system permission.

- `permissions`: list of permissions the new user will have in `team`. Must be
  a subset of permissions held in `team` by current user.

Each invitation is valid for seven days.

[BCP47]: https://tools.ietf.org/rfc/bcp/bcp47.txt

#### Status codes

- 202: invitation has been accepted for processing. Sending invitation may still
  fail because the email might not exist.

- 400: request contained an invalid email, invalid language tag, or specified
  an unsupported language.

### `GET /api/v1/users/:id`

Return detailed information about a particular user, as a JSON object of the
[`User`](#user) model.

When request is made for the current user (`:id` is `me`) fields `permissions`
and `role.permissions` are always present, regardless of user's permissions.

### `PUT /api/v1/users/:id`

Modify a user. Accepts either `application/x-www-form-urlencoded` or a JSON
object with following fields/properties:

```
{
    language: string?,
    permissions: SystemPermission[]?,
    name: string?,
}
```

- `language`: a [BCP 47][BCP47] language tag naming language to which user's
  preferred language will be set;

- `permissions`: a set of permissions to assign to the user. Any permissions
  which user already has but which are not present in this field will be
  removed.

  This field can only be used by a user with the [`user:edit-permissions`](
  ../#p-user-edit-permissions) system permission.

- `name`: user's name;

All fields may be omitted, in which case no action is taken. Fields `language`
and `name` can only be used by current user (`:id` is `me`) or in an elevated
session with the [`user:edit`](../#p-user-edit) permission.

#### Status codes

- 200: user was modified. Response contains a JSON object of the [`User`](#user)
  model, describing the user with changes applied.

- 400: `language` was not a valid language tag or did not name a supported
  language, or `permissions` did not name existing permissions, or `role` did
  not name an existing role.

### `GET /api/v1/users/:id/drafts`

Get list of all drafts a user has access to, as a JSON array of objects of the
[`Draft`](../drafts.md#Draft) model.

This list will only include drafts in teams in which current user has the
[`editing-process:manage`](../#p-editing-process-manage) permission.

### `PUT /api/v1/users/me/password`

Change current user's password. Accepts either
`application/x-www-form-urlencoded` or a JSON object, with following
fields/properties:

```
{
    current: string,
    new: string,
    new2: string,
}
```

- `current`: user's current password;

- `new` and `new2`: new password.

#### Status codes

- 204: password was changed.

- 400: `new` and `new2` did not match.

- 400 `user:change-password:empty`: user tried to change their password to an
  empty one.

- 403: `current` was not accepted as user's current password.

### `GET /api/v1/users/me/session`

Get details about current session. Returns a JSON object with following
properties:

```
{
    expires: date,
    is_elevated: boolean,
    permissions: Permission[],
}
```

- `expires`: date and time at which this session will expire and user will have
  to re-authenticate;

- `is_elevated`: is this an elevated session;

- `permissions`: set of permissions the user has in this session.

### `POST /reset`

Reset password without logging in by fulfilling a password reset token, or
create a new password reset token. Accepts an
`application/x-www-form-urlencoded` with either following fields

```
{
    email: string,
}
```

or following fields

```
{
    password: string,
    password1: string,
    token: string,
}
```

- `email`: email to which to send reset token;

- `password` and `password1`: user's new password;

- `token`: password reset token to fulfil.

In the first form a new password reset token will be crated for specified user
and sent to them by email. In the second form the token will be fulfilled,
changing user's password and creating a new session for them.

#### Status codes returned

- 400 `password:reset:expired`: the reset `token` in use is already expired.

- 400 `password:reset:invalid`: the reset `token` is not valid.

- 400 `password:reset:passwords-dont-match`: `password` and `password1` don't
  match.

### `POST /register`

Fulfil an invitation and crate a new user. Accepts an
`application/x-www-form-data` with following fields:

```
{
    email: string,
    name: string,
    password: string,
    password1: string,
    invite: string,
    language: string,
}
```

- `email`: user's email address. Must be the one the invitation was sent to;

- `name`: user's name;

- `password` and `password1`: user's password;

- `invite`: invitation code;

- `language`: user's preferred language.

#### Status codes

- 400 `invitation:expired`: specified `invite` has expired.

- 400 `invitation:invalid`: specified `invite` is not valid.

- 400 `user:new:empty-name`: user tried to register with an empty `name`.

- 400 `user:new:empty-password`: user tried to register with an empty
  `password`.

- 400 `user:new:exists`: there is already a user registered for this email
  address.



## Common status codes #########################################################

- 400 `user:authenticate:bad-password`: specified password did not match the one
  on record.

- 400 `user:password:bad-confirmation`: when setting or changing a password, the
  new password and its confirmation didn't match.

- 404 `user:not-found`: specified `:id` doesn't match any existing user.
