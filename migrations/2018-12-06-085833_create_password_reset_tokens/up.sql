create table password_reset_tokens (
    id      serial      primary key,
    "user"  integer     not null references users(id),
    expires timestamp   not null
);
