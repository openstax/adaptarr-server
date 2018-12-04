create table users (
    id          serial  primary key,
    email       varchar not null unique,
    name        varchar not null,
    password    bytea   not null,
    salt        bytea   not null,
    is_super    boolean not null
);
