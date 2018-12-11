create table drafts (
    module      uuid    not null references modules(id),
    "user"      integer not null references users(id),
    document    integer not null references documents(id),

    primary key (module, "user")
);
