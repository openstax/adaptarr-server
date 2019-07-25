# Book management endpoints



## Models ######################################################################

### `Book`

```
{
    id: uuid,
    title: string,
}
```

This model is used throughout the API to describe books. The fields are

- `id`: book's UUID;

- `title`: book's title.

### `Tree`

```
{
    number: number,
    title: string,
    kind: 'module' | 'group',
    id: uuid,
    parts: Tree[],
}
```

This model describes structure of a book. The fields are

- `number`: part or sub-tree's ID;

- `title`: part or sub-tree's title;

- `kind`: part's kind;

- `id`: UUID of the module this part represents. Only present when `kind` is
  `'module'`.

- `parts`: ordered list of parts in this sub-tree. Only present when `kind` is
  `'group'`.

### `NewTree`

Either

```
{
    title: string?,
    module: uuid,
}
```

or

```
{
    title: string,
    parts: NewTree[]?,
}
```

This model describes structure of a new book part. The fields are:

- `title`: part's title. In the first form this field is optional, and when
  empty will be derived from title of the module named by `module`.

- `module`: UUID of a module this new part will represent;

- `parts`: ordered list of new parts to be created as children of this part.

### `BookPart`

```
{
    number: number,
    title: string,
    kind: 'module' | 'group',
    id: uuid,
    parts: number[],
}
```

- `number`: part's ID;

- `title`: part's title'

- `kind`: part's kind;

- `id`: UUID of the module this part represents. Only present when `kind` is
  `'module'`.

- `parts`: ordered list of IDs of parts in this sub-tree. Only present when
  `kind` is `'group'`.

### `:id`

In endpoint names `:id` stands for a book's UUID, formatted as a string.



## Endpoints ###################################################################

### `GET /api/v1/books`

Return list of all books in the system, as a JSON array of objects of the
[`Book`](#book) model.

### `POST /api/v1/books`

Create a new book. Accepts either a JSON object or a `multipart/form-data`, both
formats including following fields/properties:

```
{
    title: string,
}
```

- `title`: new book's title.

In the first form (a JSON object) an empty book will be created. The second form
(`multipart/form-data`) requires an additional field, `file`, containing a CNX
ZIP export of a collection, and creates a book from that export (including
creation of any modules contained in it).

This endpoint is only available in elevated sessions with the [`book:edit`](
../#p-book-edit) permission.

#### Status codes

- 201: book was created. Response contains a JSON object of the [`Book`](#book)
  model, describing the new book.

This endpoint can also return all error codes returned by the
[`PUT /api/v1/books/:id`](#put-apiv1booksid) endpoint.

### `GET /api/v1/books/:id`

Get detailed information about a particular book, as a JSON object of the
[`Book`](#book) model.

### `PUT /api/v1/books/:id`

Modify a book. Accepts either a JSON object or a raw ZIP file.

The JSON object in the first form has following properties:

```
{
    title: string,
}
```

- `title`: book's new title;

This form modifies book's properties without affecting its contents. The second
form (raw ZIP upload) doesn't touch book's properties and instead replaces its
contents with a new book imported from a CNX collection export.

This endpoint is only available in elevated sessions with the [`book:edit`](
../#p-book-edit) permission.

#### Status codes

- 200: book was updated. Response contains a JSON object of the [`Book`](#book)
  model, describing the book witch changes applied.

- 400 `import:invalid-xml`: `collection.xml` is not a valid ColXML file.

- 400 `import:zip:collection-xml-missing`: ZIP archive doesn't include
  `collection.xml`

- 400 `import:zip:invalid`: request doesn't contain valid ZIP archive.

This endpoint can also return all error codes returned by the
[`PUT /api/v1/modules/:id`](#../modules.md#put-apiv1modulesid) endpoint.

### `DELETE /api/v1/books/:id`

Delete a book.

This endpoint is only available in elevated sessions with the [`book:edit`](
../#p-book-edit) permission.

#### Status codes

- 204: book was deleted.

### `GET /api/v1/books/:id/parts`

Get detailed description of a book's structure and contents, as a JSON object of
the [`Tree`](#tree) model.

### `POST /api/v1/books/:id/parts`

Create a new part of a book. Accepts a JSON object of the [`NewTree`](#newtree)
model wit additional properties:

```
{
    parent: number,
    index: number,
}
```

- `parent` number of a part of this book inside which this new part is to be
  created;

- `index`: index inside `parent` at which this new part is to be created.

This endpoint is only available in elevated sessions with the [`book:edit`](
../#p-book-edit) permission.

#### Status codes

- 201: part was created.

- 400 `bookpart:create-part:is-module`: could not create a new part because the
  `parent` selected is a module and can't contain other parts.

### `GET /api/v1/books/:id/parts/:number`

Get detailed information about a particular part of a book, as a JSON object of
the [`BookPart`](#bookpart) model.

### `DELETE /api/v1/books/:id/parts/:number`

Delete a part of a book.

#### Status codes

- 204: part was deleted.

- 400 `bookpart:delete:is-root`: book part could not be deleted because it's the
  root part of a book.

### `PUT /api/v1/books/:id/parts/:number`

Modify a part of a book. Accepts a JSON object with following properties

```
{
    title: string?,
    parent: number?,
    index: number?,
}
```

- `title`: part's new title;

- `parent`: ID of the part into which to move this part;

- `index`: index in `parent` to which to move this part.

All fields may be omitted, in which case no action is taken. Fields `parent` and
`index` must be used together. This endpoint is only available in elevates
sessions with the [`book:edit`](../#p-book-edit) permission.

#### Status codes returned

- 204: book part was changed.



## Common status codes #########################################################

- 404 `book:not-found`: specified `:id` doesn't match any existing book.
