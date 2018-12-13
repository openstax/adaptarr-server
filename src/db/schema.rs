table! {
    book_parts (book, id) {
        book -> Uuid,
        id -> Int4,
        title -> Varchar,
        module -> Nullable<Uuid>,
        parent -> Int4,
        index -> Int4,
    }
}

table! {
    books (id) {
        id -> Uuid,
        title -> Varchar,
    }
}

table! {
    document_files (id) {
        id -> Int4,
        document -> Int4,
        name -> Varchar,
        file -> Int4,
    }
}

table! {
    documents (id) {
        id -> Int4,
        name -> Varchar,
        index -> Int4,
    }
}

table! {
    drafts (module, user) {
        module -> Uuid,
        user -> Int4,
        document -> Int4,
    }
}

table! {
    events (id) {
        id -> Int4,
        user -> Int4,
        timestamp -> Timestamp,
        kind -> Varchar,
        is_unread -> Bool,
        data -> Bytea,
    }
}

table! {
    files (id) {
        id -> Int4,
        mime -> Varchar,
        path -> Varchar,
        hash -> Bytea,
    }
}

table! {
    invites (id) {
        id -> Int4,
        email -> Varchar,
        expires -> Timestamp,
    }
}

table! {
    modules (id) {
        id -> Uuid,
        document -> Int4,
        assignee -> Nullable<Int4>,
    }
}

table! {
    module_versions (module, document) {
        module -> Uuid,
        document -> Int4,
        version -> Timestamp,
    }
}

table! {
    password_reset_tokens (id) {
        id -> Int4,
        user -> Int4,
        expires -> Timestamp,
    }
}

table! {
    sessions (id) {
        id -> Int4,
        user -> Int4,
        expires -> Timestamp,
        last_used -> Timestamp,
        is_super -> Bool,
    }
}

table! {
    users (id) {
        id -> Int4,
        email -> Varchar,
        name -> Varchar,
        password -> Bytea,
        salt -> Bytea,
        is_super -> Bool,
    }
}

joinable!(book_parts -> books (book));
joinable!(book_parts -> modules (module));
joinable!(document_files -> documents (document));
joinable!(document_files -> files (file));
joinable!(documents -> files (index));
joinable!(drafts -> documents (document));
joinable!(drafts -> modules (module));
joinable!(drafts -> users (user));
joinable!(events -> users (user));
joinable!(module_versions -> documents (document));
joinable!(module_versions -> modules (module));
joinable!(modules -> documents (document));
joinable!(modules -> users (assignee));
joinable!(password_reset_tokens -> users (user));
joinable!(sessions -> users (user));

allow_tables_to_appear_in_same_query!(
    book_parts,
    books,
    document_files,
    documents,
    drafts,
    events,
    files,
    invites,
    modules,
    module_versions,
    password_reset_tokens,
    sessions,
    users,
);
