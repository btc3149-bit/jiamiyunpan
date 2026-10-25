use actix_web::{route, web, HttpRequest, HttpResponse};
use auth::data::authenticated::Authenticated;
use context::Context;
use entity::Uuid;
use error::AppResult;

use crate::{data::update::Update, repository::Repository};

/// Update app link properties — expiry and protection (password, download
/// cap). Omitted protection fields keep their current value.
///
/// Response: [crate::data::app_link::AppLink]
#[route("/api/links/{link_id}", method = "PUT")]
pub(crate) async fn update(
    req: HttpRequest,
    context: web::Data<Context>,
    authenticated: Authenticated,
    data: web::Json<Update>,
) -> AppResult<HttpResponse> {
    let context = context.into_inner();
    let repository = Repository::new(&context);
    let update = data.into_inner().into_value()?;

    let id: Uuid = util::actix::path_var(&req, "link_id")?;

    let response = repository.update(id, authenticated.user.id, update).await?;

    Ok(HttpResponse::Ok().json(response))
}
