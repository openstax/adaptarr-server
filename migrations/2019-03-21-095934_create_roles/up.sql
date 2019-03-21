create table roles (
    id          serial  primary key,
    name        varchar not null unique,
    permissions integer not null
);

alter table users
add column "role" integer references roles(id);
