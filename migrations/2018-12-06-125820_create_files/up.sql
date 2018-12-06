create table files (
    id      serial  primary key,
    mime    varchar not null,
    path    varchar not null unique,
    hash    bytea   not null
);
