create table book_parts (
    book    uuid    not null references books(id) on delete cascade,
    id      serial  not null,
    title   varchar not null,
    module  uuid    references modules(id),
    parent  integer not null,
    index   integer not null,

    primary key (book, id),
    foreign key (book, parent) references book_parts(book, id) on delete cascade,
    unique (book, parent, index)
);

-- Create group 0 in every existing book.
insert into book_parts (book, id, title, parent, index)
select id, 0, title, 0, 0
from books;

-- When a new book is created, create a group 0 for it.
create function create_default_group_for_new_books()
returns trigger
as $$
begin
    insert into book_parts (book, id, title, parent, index)
    values (new.id, 0, new.title, 0, 0);

    return null;
end
$$ language plpgsql;

create trigger create_default_group_for_new_books
after insert on books
for each row
execute procedure create_default_group_for_new_books();

-- Prevent deletion of group 0
create function prevent_deletion_of_group_zero()
returns trigger
as $$
begin
    if old.id = 0 and exists (select 1 from books where id = old.book) then
        raise exception 'Cannot delete group 0 of a book';
    end if;

    return null;
end
$$ language plpgsql;

create trigger prevent_deletion_of_group_zero
before delete on book_parts
for each row
execute procedure prevent_deletion_of_group_zero();
