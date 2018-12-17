create or replace function prevent_deletion_of_group_zero()
returns trigger
as $$
begin
    if old.id = 0 and exists (select 1 from books where id = old.book) then
        raise exception 'Cannot delete group 0 of a book';
    end if;

    return old;
end
$$ language plpgsql;
