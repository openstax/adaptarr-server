create function update_module_history()
returns trigger
as $$
begin
    insert into module_versions (module, document, version)
    values (new.id, new.document, now());

    return null;
end
$$ language plpgsql;

create trigger update_module_history
after insert or update on modules
for each row
execute procedure update_module_history();
