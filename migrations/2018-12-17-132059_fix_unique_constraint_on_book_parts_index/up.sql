alter table book_parts
drop constraint book_parts_book_parent_index_key,
add constraint book_parts_book_parent_index_key
    unique (book, parent, index)
    deferrable initially deferred;
