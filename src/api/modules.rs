use actix_web::{App, HttpRequest, HttpResponse, http::Method};

/// Configure routes.
pub fn routes(app: App<()>) -> App<()> {
    app
        .resource("/modules/{id}", |r| {
            r.get().f(get_module);
            r.post().f(crete_draft);
            r.delete().f(delete_module);
        })
        .resource("/modules/{id}/comments", |r| {
            r.get().f(list_comments);
            r.post().f(add_comment);
        })
        .route("/modules/{id}/files", Method::GET, list_files)
        .route("/modules/{id}/files/{name}", Method::GET, get_file)
}

/// Get a module by ID.
///
/// ## Method
///
/// ```
/// GET /modules/:id
/// ```
pub fn get_module(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Create a new draft of a module.
///
/// ## Method
///
/// ```
/// POST /modules/:id
/// ```
pub fn crete_draft(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Delete a module
///
/// ## Method
///
/// ```
/// DELTE /modules/:id
/// ```
pub fn delete_module(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Get comments on a module.
///
/// ## Method
///
/// ```
/// GET /modules/:id/comments
/// ```
pub fn list_comments(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Add a comment to a module
///
/// ## Method
///
/// ```
/// POST /modules/:id/comments
/// ```
pub fn add_comment(_req: &HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// List files in a module.
///
/// ## Method
///
/// ```
/// GET /modules/:id/files
/// ```
pub fn list_files(_req: HttpRequest) -> HttpResponse {
    unimplemented!()
}

/// Get a file from a module.
///
/// ## Method
///
/// ```
/// GET /modules/:id/files/:name
/// ```
pub fn get_file(_req: HttpRequest) -> HttpResponse {
    unimplemented!()
}
