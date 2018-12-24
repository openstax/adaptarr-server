create function update_book_parts_from_books()
returns trigger
as $$
begin
    update book_parts
    set title = new.title
    where book = new.id
      and id = 0;

    return new;
end
$$ language plpgsql;

create trigger update_book_parts_from_books
after update of title on books
for each row
execute procedure update_book_parts_from_books();
