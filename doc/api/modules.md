# Module management endpoints



## Models ######################################################################

### `Module`

```
{
    id: uuid,
    title: string,
    language: string,
    process: {
        process: number,
        version: number,
        step: {
            id: number,
            name: string,
        },
    }?,
}
```

This model is used throughout the API to describe modules. The fields are

- `id`: module's UUID;

- `title`: module's title;

- `language`: module's language;

- `process`: if there is a draft derived from this module, this field contains
  information about the editing process active for that draft. Otherwise this
  field is `null`.

- `process.process`: process's ID;

- `process.version`: process's version;

- `process.step.id`: ID of the step this draft is currently at.

- `process.step.name`: `process.step.id`'s name.



## Endpoints ###################################################################

### `GET /api/v1/modules`

Return list of all modules in the system, as a JSON array of objects of the
[`Module`](#module) model.

### `POST /api/v1/modules`

Create a new module. Accepts either a JSON object or a `mutlipart/form-data`,
both formats include following fields/properties:

```
{
    title: string,
}
```

- `title`: new module's title.

The first form (JSON object) also accepts following properties:

```
{
    language: string,
}
```

- `language`: a [BCP 47][BCP47] language tag naming the language to use for the
  new module.

This form will create an empty module.

The second form (`multipart/form-data`) requires an additional field, `file`,
containing a CNX ZIP export of a module, and creates a module from that export.

This endpoint is only available in elevated sessions with the [`module:edit`](
../#p-module-edit) permission.

[BCP47]: https://tools.ietf.org/rfc/bcp/bcp47.txt

#### Status codes

- 201: module was created. Response contains a JSON object of the
  [`Module`](#module) model, describing the new module.

This endpoint can also return all error codes returned by the
[`PUT /api/v1/modules/:id`](#put-apiv1modulesid) endpoint.

### `GET /api/v1/modules/:id`

Return detailed information about a particular module, as a JSON object of the
[`Module`](#module) model.

### `POST /api/v1/modules/:id`

Begin an editing process for a module. Accepts either an
`application/x-www-form-data` of a JSON object with following fields/properties:

```
{
    process: number,
    slots: [number, number][],
}
```

- `process`: ID of a version of the process to use;

- `slots`: list of mappings from slot IDs to user IDs, specifying which users to
  assign to which slots at the beginning of a process.

This endpoint is only available in elevated sessions with the
[`process:manage`](../#p-process-manage) permission.

#### Status code

- 201: a draft was created. Response contains a JSON object of the
  [`Module`](#module) model, describing the newly created draft.

- 400 `draft:create:bad-slot`: one of specified `slots` is not part of the
  specified `process`.

- 400 `draft:create:exists`: there already exists a draft for this module.

### `PUT /api/v1/modules/:id`

Modify a module. Accepts a ZIP file containing a CNX ZIP export of a module, and
replaces this module's content from that export.

This endpoint is only available in elevated sessions with the [`module:edit`](
../#p-module-edit) permission.

#### Status code

- 200: module was updated. Response contains a JSON object of the
  [`Module`](#module) model, describing the module with changes applied.

- 400 `import:invalid-xml`: `idnex.cnxml` contains invalid CNXML.

- 400 `import:zip:index-missing`: ZIP archive does not include an `index.cnxml`.

- 400 `import:zip:invalid`: request does not contain a valid ZIP container.

- 400 `module:replace:has-draft`: module's contents cannot be replaces as there
  exists a draft of this module.

### `GET /api/v1/modules/:id/files`

Get list of files in this module, excluding `index.cnxml`, as a JSON array of
objects containing following properties:

```
{
    name: string,
    mime: string,
}
```

- `name`: file's name;

- `mime`: file's MIME type.

### `GET /api/v1/modules/:id/files/:name`

Get contents of a particular file in a module.

#### Status codes

- 404 `fild:not-found`: no file with such name could be found in this module.

### `GET /api/v1/modules/:id/xref-targets`

Get list of possible cross-reference targets within a module. Returns a JSON
list of objects with following properties:

```
{
    id: string,
    type: string,
    description: string?,
    context: string?,
    counter: number,
}
```

- `id`: target's ID;

- `type`: target's type (corresponds roughly to CNXML element's name);

- `description`: description of this target (for example caption of a figure);

- `context`: if present, ID of another reference target containing this one;

- `counter`: value of a target type-specific counter value.

#### Status codes

- 503 `module:xref:not-ready`: list of cross-references has not yet been
  computed.

### `GET /api/v1/modules/:id/books`

Get list of books containing this module, as a JSON array of UUIDs of books.



## Common status codes #########################################################

- 404 `module:not-found`: specified `:id` doesn't match any existing module.
