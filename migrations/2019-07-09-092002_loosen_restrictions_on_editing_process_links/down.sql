alter table edit_process_links
drop constraint edit_process_links_from_to_slot_key,
add unique ("to", slot);
