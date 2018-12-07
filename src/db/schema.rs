table! {
    documents (id) {
        id -> Int4,
        name -> Varchar,
        index -> Int4,
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

joinable!(documents -> files (index));
joinable!(module_versions -> documents (document));
joinable!(module_versions -> modules (module));
joinable!(modules -> documents (document));
joinable!(modules -> users (assignee));
joinable!(password_reset_tokens -> users (user));
joinable!(sessions -> users (user));

allow_tables_to_appear_in_same_query!(
    documents,
    files,
    invites,
    modules,
    module_versions,
    password_reset_tokens,
    sessions,
    users,
);
