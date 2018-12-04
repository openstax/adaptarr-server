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

joinable!(sessions -> users (user));

allow_tables_to_appear_in_same_query!(
    sessions,
    users,
);
