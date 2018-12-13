create table events (
    id          serial      primary key,
    "user"      integer     not null references users(id) on delete cascade,
    timestamp   timestamp   not null default now(),
    kind        varchar     not null,
    is_unread   bool        not null default true,
    data        bytea       not null
);
