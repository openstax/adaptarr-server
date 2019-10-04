table! {
    audit_log (id) {
        id -> Int4,
        timestamp -> Timestamptz,
        actor -> Nullable<Int4>,
        context -> Varchar,
        context_id -> Nullable<Int4>,
        context_uuid -> Nullable<Uuid>,
        kind -> Varchar,
        data -> Bytea,
    }
}

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
        team -> Int4,
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
        team -> Int4,
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
        team -> Int4,
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
    edit_process_slot_roles (slot, role) {
        slot -> Int4,
        role -> Int4,
    }
}

table! {
    edit_process_slots (id) {
        id -> Int4,
        process -> Int4,
        name -> Varchar,
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
        version -> Timestamptz,
        start -> Int4,
    }
}

table! {
    events (id) {
        id -> Int4,
        user -> Int4,
        timestamp -> Timestamptz,
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
        expires -> Timestamptz,
        role -> Nullable<Int4>,
        team -> Int4,
        permissions -> Int4,
        user -> Nullable<Int4>,
    }
}

table! {
    modules (id) {
        id -> Uuid,
        document -> Int4,
        team -> Int4,
    }
}

table! {
    module_versions (module, document) {
        module -> Uuid,
        document -> Int4,
        version -> Timestamptz,
    }
}

table! {
    password_reset_tokens (id) {
        id -> Int4,
        user -> Int4,
        expires -> Timestamptz,
    }
}

table! {
    resources (id) {
        id -> Uuid,
        name -> Varchar,
        file -> Nullable<Int4>,
        parent -> Nullable<Uuid>,
        team -> Int4,
    }
}

table! {
    roles (id) {
        id -> Int4,
        name -> Varchar,
        permissions -> Int4,
        team -> Int4,
    }
}

table! {
    sessions (id) {
        id -> Int4,
        user -> Int4,
        expires -> Timestamptz,
        last_used -> Timestamptz,
        is_elevated -> Bool,
    }
}

table! {
    team_members (user, team) {
        team -> Int4,
        user -> Int4,
        permissions -> Int4,
        role -> Nullable<Int4>,
    }
}

table! {
    teams (id) {
        id -> Int4,
        name -> Varchar,
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

joinable!(audit_log -> users (actor));
joinable!(book_parts -> books (book));
joinable!(book_parts -> modules (module));
joinable!(books -> teams (team));
joinable!(document_files -> documents (document));
joinable!(document_files -> files (file));
joinable!(documents -> files (index));
joinable!(draft_slots -> drafts (draft));
joinable!(draft_slots -> edit_process_slots (slot));
joinable!(draft_slots -> users (user));
joinable!(drafts -> documents (document));
joinable!(drafts -> edit_process_steps (step));
joinable!(drafts -> modules (module));
joinable!(drafts -> teams (team));
joinable!(edit_process_links -> edit_process_slots (slot));
joinable!(edit_process_slot_roles -> edit_process_slots (slot));
joinable!(edit_process_slot_roles -> roles (role));
joinable!(edit_process_slots -> edit_process_versions (process));
joinable!(edit_process_step_slots -> edit_process_slots (slot));
joinable!(edit_process_step_slots -> edit_process_steps (step));
joinable!(edit_process_versions -> edit_processes (process));
joinable!(edit_processes -> teams (team));
joinable!(events -> users (user));
joinable!(invites -> roles (role));
joinable!(invites -> teams (team));
joinable!(invites -> users (user));
joinable!(module_versions -> documents (document));
joinable!(module_versions -> modules (module));
joinable!(modules -> documents (document));
joinable!(modules -> teams (team));
joinable!(password_reset_tokens -> users (user));
joinable!(resources -> files (file));
joinable!(resources -> teams (team));
joinable!(roles -> teams (team));
joinable!(sessions -> users (user));
joinable!(team_members -> roles (role));
joinable!(team_members -> teams (team));
joinable!(team_members -> users (user));
joinable!(xref_targets -> documents (document));

allow_tables_to_appear_in_same_query!(
    audit_log,
    book_parts,
    books,
    document_files,
    documents,
    drafts,
    draft_slots,
    edit_processes,
    edit_process_links,
    edit_process_slot_roles,
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
    resources,
    roles,
    sessions,
    team_members,
    teams,
    users,
    xref_targets,
);
