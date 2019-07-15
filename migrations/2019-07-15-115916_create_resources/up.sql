create table resources (
    id      uuid    primary key,
    name    varchar not null,
    file    integer references files(id),
    parent  uuid    references resources(id),

    unique (name, parent)
);
