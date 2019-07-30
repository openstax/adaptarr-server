alter table edit_process_links
drop constraint edit_process_links_to_slot_key,
add unique ("from", "to", slot);
