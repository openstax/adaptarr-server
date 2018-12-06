create table documents (
    id      serial  primary key,
    name    varchar not null,
    index   integer not null references files(id)
);
