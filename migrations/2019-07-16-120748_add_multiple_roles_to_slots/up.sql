create table edit_process_slot_roles (
    slot    integer not null references edit_process_slots(id),
    role    integer not null references roles(id),

    primary key (slot, role)
);

insert into edit_process_slot_roles (slot, role)
select id as slot, role
from edit_process_slots
where role is not null;

alter table edit_process_slots
drop column role;
