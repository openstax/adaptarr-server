# API endpoints

Adaptarr! uses a RESTful API located at `/api/v1`. In order to access any of the
API endpoints a session needs to be created, usually by a user logging in at
`/login`.

Access to different resources may be guarded by permissions, which specify what
operations a user can do, and how detailed information they can see. There are
two kinds of permissions: _normal_ permissions are applied to a session when it
is created, and govern only non-mutable operations. _Elevated_ permissions are
only granted to elevated sessions, and are required to modify resources. Any
session can be elevated by going to `/elevate`. A full list of permissions is
available in [Permissions](#permissions).

Different API object are documented in separate documents:

- [Books](./books.md) are an ordered collection of modules.

- [Drafts](./drafts.md) are a version of modules that allows modification.

- [Events](./events.md) are notifications which a user receives when something
  concerning them occurs.

- [Modules](./modules.md) are self-contained documents which serve as the basic
  building block for books.

- [Editing processes](./processes.md) prescribe a set process which a draft must
  follow when being changed.

- [Resources](./resources.md) are files (or collections thereof) which contain
  information useful to users.

- [Roles](./roles.md) are groups of users sharing a single role.

- [Users](./users.md)

When a request to an API endpoint fails with a 4xx status code (client error),
and explanation of the error returned as a JSON object with following
properties:

```
{
    error: string,
    raw: string,
}
```

- `error`: a code describing type of the error.

- `raw`: a message (in English) describing this error. Note that this field is
  generally not intended to be displayed to the user, and is included mostly to
  aid in debugging.



## Permissions

Permissions in API are represented as string with following values (such string
is called the <a name="permission"></a> `Permission` model). A set of
permissions is represented as an array of those strings.

- <a name="p-user-invite"></a> `user:invite` allows inviting new users to the
  platform.

- <a name="p-user-delete"></a> `user:delete` allows removing existing users from
  the platform.

- <a name="p-user-edit-permissions"></a> `user:edit-permissions` allows changing
  other user's permissions.

- <a name="p-user-assign-role"></a> `user:assign-role` allows assigning roles to
  users.

- <a name="p-user-edit"></a> `user:edit` allows editing other users.

- <a name="p-book-edit"></a> `book:edit` allows creating, editing, and removing
  books.

- <a name="p-module-edit"></a> `module:edit` allows creating, editing, and
  removing modules.

- <a name="p-role-edit"></a> `role:edit` allows creating, editing, and removing
  roles.

- <a name="p-editing-process-edit"></a> `editing-process:edit` allows creating,
  editing, and removing editing processes.

- <a name="p-editing-process-manage"></a> `editing-process:manage` allows
  starting and managing editing processes for modules.

- <a name="p-resources-manage"></a> `resources:manage` allows creating, editing,
  and removing resources.



## Common status codes

- 400 `locale:not-found`: in requests which accept a [BCP 47][BCP47] language
  tag, the language tag did not name a supported language.

- 401 `user:session:required`: a session is required to access this resource
  but no session was present in the request, or the session present was expired.

- 403 `user:insufficient-permissions`: access to a resource was denied because
  current session doesn't have necessary permissions.

- 403 `user:session:rejected`: a session did not have necessary permissions to
  access this resource.

[BCP47]: https://tools.ietf.org/rfc/bcp/bcp47.txt
