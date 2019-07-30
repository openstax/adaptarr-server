create table audit_log (
    id              serial      primary key,
    timestamp       timestamp   not null default now(),
    actor           integer     references users(id),
    context         varchar     not null,
    context_id      integer,
    context_uuid    uuid,
    kind            varchar     not null,
    data            bytea       not null
);
