use actix_web::{
    HttpRequest,
    HttpResponse,
    Responder,
    http::{StatusCode, header::{ETAG, ContentDisposition, DispositionType}},
    web::{self, Data, Json, Payload, ServiceConfig},
};
use adaptarr_error::Error;
use adaptarr_models::{
    File,
    Model,
    Resource,
    ResourceFileError,
    Team,
    db::Pool,
    permissions::{ManageResources, PermissionBits, TeamPermissions},
};
use adaptarr_util::futures::void;
use adaptarr_web::{
    Created,
    Database,
    FileExt,
    FormOrJson,
    Session,
    TeamScoped,
    etag::IfMatch,
    multipart::{FromMultipart, FromStrField, Multipart},
};
use diesel::Connection as _;
use futures::{Future, Stream, future};
use serde::Deserialize;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::Result;

/// Configure routes.
pub fn configure(app: &mut ServiceConfig) {
    app
        .service(web::resource("/resources")
            .route(web::get().to(list_resources))
            .route(web::post().to(create_resource))
        )
        .service(web::resource("/resources/{id}")
            .route(web::get().to(get_resource))
            .route(web::put().to(update_resource))
        )
        .service(web::resource("/resources/{id}/content")
            .route(web::get().to(get_resource_content))
            .route(web::put().to_async(update_resource_content))
        )
    ;
}

/// List all resources.
///
/// ## Method
///
/// ```text
/// GET /resources
/// ```
fn list_resources(db: Database, session: Session)
-> Result<Json<Vec<<Resource as Model>::Public>>> {
    let resources = if session.is_elevated {
        Resource::all(&db)?
    } else {
        let user = session.user(&db)?;
        let teams = user.get_team_ids(&db)?;

        Resource::by_team(&db, &teams)?
    };

    Ok(Json(resources.get_public()))
}

#[derive(FromMultipart)]
struct NewResource {
    name: String,
    team: FromStrField<i32>,
    file: Option<NamedTempFile>,
    parent: Option<FromStrField<Uuid>>,
}

/// Create a new resource.
///
/// ## Method
///
/// ```text
/// POST /resources
/// Content-Type: multipart/form-data
/// ```
fn create_resource(
    req: HttpRequest,
    db: Database,
    session: Session,
    data: Multipart<NewResource>,
) -> Result<Created<String, Json<<Resource as Model>::Public>>> {
    let NewResource { name, team, file, parent } = data.into_inner();
    let team = Team::by_id(&db, *team)?;

    if !session.is_elevated {
        team.get_member(&db, &session.user(&db)?)?
            .permissions()
            .require(TeamPermissions::MANAGE_RESOURCES)?;
    }

    let parent = parent.map(|id| Resource::by_id(&db, *id)).transpose()?;

    let resource = db.transaction::<_, Error, _>(|| {
        let storage_path = &adaptarr_models::Config::global().storage.path;
        let file = file.map(|file| File::from_temporary(
            &db, storage_path, file, None)).transpose()?;

        Resource::create(&db, &team, &name, file.as_ref(), parent.as_ref())
            .map_err(From::from)
    })?;

    let location = format!("{}/api/v1/resources/{}",
        req.app_config().host(), resource.id);

    Ok(Created(location, Json(resource.get_public())))
}

/// Get a resource by ID.
///
/// ## Method
///
/// ```text
/// GET /resources/:id
/// ```
fn get_resource(scope: TeamScoped<Resource>)
-> Result<Json<<Resource as Model>::Public>> {
    Ok(Json(scope.resource().get_public()))
}

#[derive(Deserialize)]
struct ResourceUpdate {
    name: String,
}

/// Update a resource.
///
/// ## Method
///
/// ```text
/// PUT /resources/:id
/// ```
fn update_resource(
    db: Database,
    scope: TeamScoped<Resource, ManageResources>,
    update: FormOrJson<ResourceUpdate>,
) -> Result<Json<<Resource as Model>::Public>> {
    let mut resource = scope.into_resource();

    resource.set_name(&db, &update.name)?;

    Ok(Json(resource.get_public()))
}

/// Get file associated with a resource.
///
/// ## Method
///
/// ```text
/// GET /resources/:id/content
/// ```
fn get_resource_content(db: Database, scope: TeamScoped<Resource>)
-> Result<impl Responder> {
    let storage_path = &adaptarr_models::Config::global().storage.path;

    Ok(scope.resource()
        .get_file(&db)?
        .stream(storage_path)?
        .set_content_disposition(ContentDisposition {
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
fn update_resource_content(
    db: Database,
    pool: Data<Pool>,
    scope: TeamScoped<Resource, ManageResources>,
    if_match: IfMatch,
    payload: Payload,
) -> Box<dyn Future<Item = HttpResponse, Error = Error>> {
    let mut resource = scope.into_resource();

    if resource.is_directory() {
        return Box::new(future::err(ResourceFileError::IsADirectory.into()));
    }

    if !if_match.is_any() {
        let file = match resource.get_file(&db) {
            Ok(file) => file,
            Err(err) => return Box::new(future::err(err.into())),
        };

        if !if_match.test(&file.entity_tag()) {
            return Box::new(payload.from_err()
                .forward(void::<_, Error>())
                .map(|_| HttpResponse::new(StatusCode::PRECONDITION_FAILED)));
        }
    }

    let storage_path = &adaptarr_models::Config::global().storage.path;
    Box::new(File::from_stream((*pool).clone(), storage_path, payload, None)
        .and_then(move |file|
            resource.set_file(&db, &file)
                .map_err(From::from)
                .map(|_|
                    HttpResponse::NoContent()
                        .header(ETAG, file.entity_tag())
                        .finish()
                )
        ))
}
