use actix_web::{
    App,
    HttpMessage,
    HttpRequest,
    HttpResponse,
    Json,
    Path,
    Responder,
    http::{StatusCode, header::{ContentDisposition, DispositionType}},
};
use diesel::Connection as _;
use futures::{Future, future::{self, Either}};
use serde::Deserialize;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::{
    models::{File, resource::{Resource, PublicData as ResourceData, FileError}},
    multipart::{Multipart, FromStrField},
    permissions::ManageResources,
};
use super::{
    Error,
    RouteExt,
    State,
    session::Session,
    util::{Created, FormOrJson, IfMatch},
};

/// Configure routes.
pub fn routes(app: App<State>) -> App<State> {
    app
        .resource("/resources", |r| {
            r.get().api_with(list_resources);
            r.post().api_with(create_resource)
        })
        .resource("/resources/{id}", |r| {
            r.get().api_with(get_resource);
            r.put().api_with(update_resource);
        })
        .resource("/resources/{id}/content", |r| {
            r.get().api_with(get_resource_content);
            r.put().api_with_async(update_resource_content);
        })
}

type Result<T, E=Error> = std::result::Result<T, E>;

/// List all resources.
///
/// ## Method
///
/// ```text
/// GET /resources
/// ```
pub fn list_resources(
    state: actix_web::State<State>,
    _session: Session,
) -> Result<Json<Vec<ResourceData>>> {
    let db = state.db.get()?;
    let resources = Resource::all(&*db)?;

    Ok(Json(resources.iter().map(Resource::get_public).collect::<Vec<_>>()))
}

pub struct NewResource {
    name: String,
    file: Option<NamedTempFile>,
    parent: Option<FromStrField<Uuid>>,
}

from_multipart! {
    multipart NewResource via _NewResourceImpl {
        name: String,
        file: Option<NamedTempFile>,
        parent: Option<FromStrField<Uuid>>,
    }
}

/// Create a new resource.
///
/// ## Method
///
/// ```text
/// POST /resources
/// Content-Type: multipart/form-data
/// ```
pub fn create_resource(
    state: actix_web::State<State>,
    _session: Session<ManageResources>,
    data: Multipart<NewResource>,
) -> Result<Created<String, Json<ResourceData>>> {
    let db = state.db.get()?;
    let NewResource { name, file, parent } = data.into_inner();
    let parent = parent.map(|id| Resource::by_id(&*db, *id)).transpose()?;

    let resource = db.transaction::<_, Error, _>(|| {
        let file = file.map(|file| File::from_temporary(
            &*db, &state.config.storage, file, None)).transpose()?;

        Resource::create(&*db, &name, file.as_ref(), parent.as_ref())
            .map_err(From::from)
    })?;

    let location = format!("{}/api/v1/resources/{}",
        state.config.server.domain, resource.id);

    Ok(Created(location, Json(resource.get_public())))
}

/// Get a resource by ID.
///
/// ## Method
///
/// ```text
/// GET /resources/:id
/// ```
pub fn get_resource(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<Uuid>,
) -> Result<Json<ResourceData>> {
    let db = state.db.get()?;
    let resource = Resource::by_id(&*db, *id)?;

    Ok(Json(resource.get_public()))
}

#[derive(Deserialize)]
pub struct ResourceUpdate {
    pub name: String,
}

/// Update a resource.
///
/// ## Method
///
/// ```text
/// PUT /resources/:id
/// ```
pub fn update_resource(
    state: actix_web::State<State>,
    _session: Session<ManageResources>,
    id: Path<Uuid>,
    update: FormOrJson<ResourceUpdate>,
) -> Result<Json<ResourceData>> {
    let db = state.db.get()?;
    let mut resource = Resource::by_id(&*db, *id)?;
    let update = update.into_inner();

    resource.set_name(&*db, &update.name)?;

    Ok(Json(resource.get_public()))
}

/// Get file associated with a resource.
///
/// ## Method
///
/// ```text
/// GET /resources/:id/content
/// ```
pub fn get_resource_content(
    state: actix_web::State<State>,
    _session: Session,
    id: Path<Uuid>,
) -> Result<impl Responder> {
    let db = state.db.get()?;
    let resource = Resource::by_id(&*db, *id)?;
    let file = resource.get_file(&*db)?.stream(&state.config)?;

    Ok(file.set_content_disposition(ContentDisposition {
        disposition: DispositionType::Inline,
        parameters: vec![],
    }))
}

/// Change contents of a resource.
///
/// ## Method
///
/// ```text
/// PUT /resources/:id/content
/// ```
pub fn update_resource_content(
    req: HttpRequest<State>,
    state: actix_web::State<State>,
    _session: Session<ManageResources>,
    id: Path<Uuid>,
    if_match: IfMatch,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let db = match state.db.get() {
        Ok(db) => db,
        Err(err) => return Either::A(future::err(err.into())),
    };

    let mut resource = match Resource::by_id(&*db, *id) {
        Ok(resource) => resource,
        Err(err) => return Either::A(future::err(err.into())),
    };

    if resource.is_directory() {
        return Either::A(future::err(FileError::IsADirectory.into()));
    }

    if !if_match.is_any() {
        let file = match resource.get_file(&*db) {
            Ok(file) => file,
            Err(err) => return Either::A(future::err(err.into())),
        };

        if !if_match.test(&file.entity_tag()) {
            return Either::A(future::ok(
                HttpResponse::new(StatusCode::PRECONDITION_FAILED)));
        }
    }

    let storage = state.config.storage.path.clone();

    Either::B(File::from_stream(state.db.clone(), storage, req.payload(), None)
        .and_then(move |file| resource.set_file(&*db, &file).map_err(From::from))
        .map(|_| HttpResponse::new(StatusCode::NO_CONTENT)))
}
