drop trigger update_module_history on modules;

create trigger update_module_history
after insert or update of document on modules
for each row
execute procedure update_module_history();
