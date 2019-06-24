create table conversations (
    id              serial      primary key
);

create table conversation_members (
    conversation    integer     not null references conversations(id),
    "user"          integer     not null references users(id),

    primary key (conversation, "user")
);

create table conversation_events (
    id              serial      primary key,
    conversation    integer     not null references conversations(id),
    kind            varchar     not null,
    timestamp       timestamp   not null default now(),
    author          integer     references users(id),
    data            bytea       not null
);
