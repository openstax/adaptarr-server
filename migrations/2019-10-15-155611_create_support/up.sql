create table support_tickets (
    id              serial                      primary key,
    title           varchar                     not null,
    opened          timestamp with time zone    not null default now(),
    conversation    integer                     not null references conversations(id)
);

create table support_ticket_authors (
    ticket  integer                             not null references support_tickets(id),
    "user"  integer                             not null references users(id),

    primary key (ticket, "user")
);

alter table users
add column is_support boolean not null default false;
