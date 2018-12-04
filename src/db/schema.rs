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
