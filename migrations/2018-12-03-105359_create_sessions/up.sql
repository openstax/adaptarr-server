create table sessions (
    id          serial      primary key,
    "user"      integer     not null references users(id),
    expires     timestamp   not null,
    last_used   timestamp   not null default now(),
    is_super    boolean     not null
);
