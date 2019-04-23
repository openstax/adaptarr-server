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
        title -> Varchar,
        index -> Int4,
        xrefs_ready -> Bool,
        language -> Varchar,
    }
}

table! {
    drafts (module) {
        module -> Uuid,
        document -> Int4,
        step -> Int4,
    }
}

table! {
    draft_slots (draft, slot) {
        draft -> Uuid,
        slot -> Int4,
        user -> Int4,
    }
}

table! {
    edit_processes (id) {
        id -> Int4,
        name -> Varchar,
    }
}

table! {
    edit_process_links (from, to) {
        from -> Int4,
        to -> Int4,
        name -> Varchar,
        slot -> Int4,
    }
}

table! {
    edit_process_slots (id) {
        id -> Int4,
        process -> Int4,
        name -> Varchar,
        role -> Nullable<Int4>,
        autofill -> Bool,
    }
}

table! {
    edit_process_steps (id) {
        id -> Int4,
        process -> Int4,
        name -> Varchar,
    }
}

table! {
    edit_process_step_slots (step, slot, permission) {
        step -> Int4,
        slot -> Int4,
        permission -> crate::db::types::Slot_permission,
    }
}

table! {
    edit_process_versions (id) {
        id -> Int4,
        process -> Int4,
        version -> Timestamp,
        start -> Int4,
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
    roles (id) {
        id -> Int4,
        name -> Varchar,
        permissions -> Int4,
    }
}

table! {
    sessions (id) {
        id -> Int4,
        user -> Int4,
        expires -> Timestamp,
        last_used -> Timestamp,
        is_elevated -> Bool,
        permissions -> Int4,
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
        language -> Varchar,
        permissions -> Int4,
        role -> Nullable<Int4>,
    }
}

table! {
    xref_targets (document, element) {
        document -> Int4,
        element -> Varchar,
        #[sql_name = "type"]
        type_ -> Varchar,
        description -> Nullable<Varchar>,
        context -> Nullable<Varchar>,
        counter -> Int4,
    }
}

joinable!(book_parts -> books (book));
joinable!(book_parts -> modules (module));
joinable!(document_files -> documents (document));
joinable!(document_files -> files (file));
joinable!(documents -> files (index));
joinable!(draft_slots -> drafts (draft));
joinable!(draft_slots -> edit_process_slots (slot));
joinable!(draft_slots -> users (user));
joinable!(drafts -> documents (document));
joinable!(drafts -> edit_process_steps (step));
joinable!(drafts -> modules (module));
joinable!(edit_process_links -> edit_process_slots (slot));
joinable!(edit_process_slots -> edit_process_versions (process));
joinable!(edit_process_slots -> roles (role));
joinable!(edit_process_step_slots -> edit_process_slots (slot));
joinable!(edit_process_step_slots -> edit_process_steps (step));
joinable!(edit_process_versions -> edit_processes (process));
joinable!(events -> users (user));
joinable!(module_versions -> documents (document));
joinable!(module_versions -> modules (module));
joinable!(modules -> documents (document));
joinable!(password_reset_tokens -> users (user));
joinable!(sessions -> users (user));
joinable!(users -> roles (role));
joinable!(xref_targets -> documents (document));

allow_tables_to_appear_in_same_query!(
    book_parts,
    books,
    document_files,
    documents,
    drafts,
    draft_slots,
    edit_processes,
    edit_process_links,
    edit_process_slots,
    edit_process_steps,
    edit_process_step_slots,
    edit_process_versions,
    events,
    files,
    invites,
    modules,
    module_versions,
    password_reset_tokens,
    roles,
    sessions,
    users,
    xref_targets,
);
