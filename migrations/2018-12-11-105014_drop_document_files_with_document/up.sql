alter table document_files
drop constraint document_files_document_fkey,
add constraint document_files_document_fkey
    foreign key (document)
    references documents(id)
    on delete cascade;
