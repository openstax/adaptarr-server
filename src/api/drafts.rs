use actix_web::{App, HttpRequest, HttpResponse, http::Method};

/// Configure routes.
pub fn routes(app: App<()>) -> App<()> {
    app
        .resource("/drafts/{id}", |r| {
            r.get().f(get_draft);
            r.delete().f(delete_draft);
        })
        .route("/drafts/{id}/save", Method::POST, save_draft)
        .resource("/drafts/{id}/comments", |r| {
            r.get().f(list_comments);
            r.post().f(add_comment);
        })
        .route("/drafts/{id}/files", Method::GET, list_files)
        .resource("/drafts/{id}/files/{name}", |r| {
            r.get().f(get_file);
            r.put().f(update_file);
            r.delete().f(delete_file);
        })
}

/// Get a draft by ID.
///
/// ## Method
///
/// ```
/// GET /drafts/:id
/// ```
pub fn get_draft(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Delete a draft
///
/// ## Method
///
/// ```
/// DELTE /drafts/:id
/// ```
pub fn delete_draft(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Save a draft.
///
/// ## Method
///
/// ```
/// POST /drafts/:id/save
/// ```
pub fn save_draft(_req: HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Get comments on a draft.
///
/// ## Method
///
/// ```
/// GET /drafts/:id/comments
/// ```
pub fn list_comments(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Add a comment to a draft
///
/// ## Method
///
/// ```
/// POST /drafts/:id/comments
/// ```
pub fn add_comment(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// List files in a draft.
///
/// ## Method
///
/// ```
/// GET /drafts/:id/files
/// ```
pub fn list_files(_req: HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Get a file from a draft.
///
/// ## Method
///
/// ```
/// GET /drafts/:id/files/:name
/// ```
pub fn get_file(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Update a file in a draft.
///
/// ## Method
///
/// ```
/// PUT /drafts/:id/files/:name
/// ```
pub fn update_file(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Delete a file from a draft.
///
/// ## Method
///
/// ```
/// DELETE /drafts/:id/files/:name
/// ```
pub fn delete_file(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}
