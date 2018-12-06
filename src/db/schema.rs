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

joinable!(password_reset_tokens -> users (user));
joinable!(sessions -> users (user));

allow_tables_to_appear_in_same_query!(
    files,
    invites,
    password_reset_tokens,
    sessions,
    users,
);
