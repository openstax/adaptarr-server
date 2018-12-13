drop trigger update_module_history on modules;

create trigger update_module_history
after insert or update on modules
for each row
execute procedure update_module_history();
