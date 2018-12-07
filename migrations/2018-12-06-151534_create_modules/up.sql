create table modules (
    id          uuid        primary key,
    document    integer     not null references documents(id),
    assignee    integer     references users(id)
);

create table module_versions (
    module      uuid        not null references modules(id),
    document    integer     not null references documents(id),
    version     timestamp   not null,

    primary key (module, document)
);
