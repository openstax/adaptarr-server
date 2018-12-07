create table document_files (
    id          serial  primary key,
    document    integer not null references documents(id),
    name        varchar not null,
    file        integer not null references files(id),

    unique (document, name)
);
