alter table edit_process_slots
add column role integer references roles(id);

update edit_process_slots
set role = edit_process_slot_roles.role
from edit_process_slot_roles
where edit_process_slots.id = edit_process_slot_roles.slot;

drop table edit_process_slot_roles;
