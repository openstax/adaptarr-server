create or replace function duplicate_document(old_id integer)
returns integer
as $$
declare
    new_id integer;
begin
    insert into documents (title, index)
    select title, index
    from documents
    where id = old_id
    returning id into new_id;

    insert into document_files (document, name, file)
    select new_id, name, file
    from document_files
    where document = old_id;

    return new_id;
end
$$ language plpgsql
