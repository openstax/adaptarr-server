create table invites (
    id      serial      primary key,
    email   varchar     not null,
    expires timestamp   not null
);
