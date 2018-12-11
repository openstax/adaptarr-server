use diesel::sql_types::*;

sql_function!(fn duplicate_document(id: Int4) -> Int4);
