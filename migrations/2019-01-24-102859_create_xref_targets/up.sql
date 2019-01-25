create table xref_targets (
    document    integer not null references documents(id),
    element     varchar not null,
    type        varchar not null,
    description varchar,
    context     varchar,
    counter     integer not null,

    primary key (document, element)
);

alter table documents
add column xrefs_ready boolean not null default false;
