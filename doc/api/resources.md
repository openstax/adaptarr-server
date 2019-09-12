# Resource management endpoints



## Models ######################################################################

### `Resource`

```
{
    id: uuid,
    team: number,
    name: string,
    parent: uuid?,
    kind: 'file' | 'directory',
}
```

This model is used throughout the API to describe resources. The fields are

- `id`: resource's UUID;

- `team`: ID of the team owning this resource;

- `name`: resource's name;

- `parent`: if not `null`, UUID of the resource directory in which this one is
  located;

- `kind`: whether this resource is a file (`'file'`) and has [content](
  #get-apiv1resourcesidcontent), or a directory (`'directory'`) and contains
  other resources.



## Endpoints ###################################################################

### `GET /api/v1/resources`

Return list of all resources in the system as a JSON array of objects of the
[`Resource`](#resource) model.

### `POST /api/v1/resources`

Create a new resource. Accepts a `multipart/form-data` with following fields

- `name`: new resource's name'

- `file`: if present, the new resource will be a file and this field contains
  its contents. Otherwise the new resource will be a directory;

- `parent`: if present, contains UUID of the resource directory in which to
  create the new one.

This endpoint is only available in elevated sessions with the
[`resources:manage`](../#p-resources-manage) permission.

#### Status codes

- 201: a resource was created. Response contains a JSON object of the
  [`Resource`](#resource) model, describing the new resource.

- 400 `resource:new:exists`: there is already a resource with the same name in
  this directory.

### `GET /api/v1/resources/:id`

Return detailed information about a resource, as a JSON object of the
[`Resource`](#resource) model.

### `PUT /api/v1/resources/:id`

Modify a resource. Accepts either an `application/x-www-form-data` or a JSON
object with following fields/properties:

```
{
    name: string,
}
```

- `name`: resource's new name.

This endpoint is only available in elevated sessions with the
[`resources:manage`](../#p-resources-manage) permission.

#### Status codes

- 200: resource was modified. Response contains a JSON object of the
  [`Resource`](#resource) model, describing the resource with changes applied.

### `GET /api/v1/resources/:id/content`

Return contents of this resource. This endpoint is only available on `'file'`
resources.


### `PUT /api/v1/resources/:id/content`

Modify contents of this resource. This endpoint is only available on `'file'`
resources.

This endpoint is only available in elevated sessions with the
[`resources:manage`](../#p-resources-manage) permission.

#### Status codes

- 204: resource contents were changed.



## Common status codes #########################################################

- 400 `resource:is-a-directory`: returned when trying to retrieve or update
  contents of a non-file resource.

- 404 `resource:not-found`: specified `:id` doesn't match any existing resource.
