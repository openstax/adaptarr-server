create table edit_processes (
    id          serial      primary key,
    name        varchar     not null unique
);

create table edit_process_versions (
    id          serial      primary key,
    process     integer     not null references edit_processes(id),
    version     timestamp   not null,

    unique (process, version)
);

create table edit_process_slots (
    id          serial      primary key,
    process     integer     not null references edit_process_versions(id),
    name        varchar     not null,
    role        integer     references roles(id),
    autofill    boolean     not null,

    unique (process, name)
);

create table edit_process_steps (
    id          serial      primary key,
    process     integer     not null references edit_process_versions(id),
    name        varchar     not null,

    unique (process, name)
);

alter table edit_process_versions
add column start integer    not null references edit_process_steps(id) deferrable;

create type slot_permission as enum (
    'view',
    'edit',
    'propose_changes',
    'accept_changes'
);

create table edit_process_step_slots (
    step        integer     not null references edit_process_steps(id),
    slot        integer     not null references edit_process_slots(id),
    permission  slot_permission not null,

    primary key (step, slot, permission)
);

create table edit_process_links (
    "from"      integer     not null references edit_process_steps(id),
    "to"        integer     not null references edit_process_steps(id),
    name        varchar     not null,
    slot        integer     not null references edit_process_slots(id),

    primary key ("from", "to"),
    unique ("from", name),
    unique ("to", slot),
    check ("from" != "to")
);
