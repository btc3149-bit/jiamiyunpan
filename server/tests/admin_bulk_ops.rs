//! Admin bulk operations: `POST /api/admin/users/bulk` and
//! `POST /api/admin/files/bulk`. The batch is never failed by one bad id —
//! every id gets its own result entry — bulk delete refuses the current
//! admin's own account, oversized batches are rejected, and disabling a
//! user flags the account (blocking future logins) and kills its sessions.

#[path = "./helpers.rs"]
mod helpers;

use actix_web::{
    http::StatusCode,
    test,
};
use jiamiyunpan::server;
use serde_json::{json, Value};

use crate::helpers::{name_tag, TestApp};

/// The first account registered on a fresh instance is the instance admin.
async fn setup() -> (impl TestApp, crate::helpers::RegisteredCurve, crate::helpers::RegisteredCurve) {
    let context = context::Context::mock_with_data_dir(Some("../data/test".to_string())).await;
    let app = test::init_service(server::app(context.clone())).await;

    let admin = helpers::register_curve25519(&app, "bulk-admin@doe.com").await;
    let bob = helpers::register_curve25519(&app, "bulk-bob@doe.com").await;

    (app, admin, bob)
}

async fn bulk_users(
    app: &impl TestApp,
    jwt: &actix_web::cookie::Cookie<'static>,
    action: &str,
    ids: &[entity::Uuid],
) -> (StatusCode, Value) {
    let req = test::TestRequest::post()
        .uri("/api/admin/users/bulk")
        .cookie(jwt.clone())
        .set_json(json!({ "action": action, "ids": ids }))
        .to_request();
    let response = test::call_service(app, req).await;
    let status = response.status();
    let body: Value = test::try_read_body_json(response).await.unwrap_or(Value::Null);
    (status, body)
}

async fn get_admin_user(
    app: &impl TestApp,
    jwt: &actix_web::cookie::Cookie<'static>,
    id: entity::Uuid,
) -> StatusCode {
    let req = test::TestRequest::get()
        .uri(&format!("/api/admin/users/{id}"))
        .cookie(jwt.clone())
        .to_request();
    test::call_service(app, req).await.status()
}

/// Create a file through the real route and return its id.
async fn create_file(
    app: &impl TestApp,
    jwt: &actix_web::cookie::Cookie<'static>,
    name_seed: &str,
) -> entity::Uuid {
    let req = test::TestRequest::post()
        .uri("/api/storage")
        .cookie(jwt.clone())
        .set_json(json!({
            "encrypted_key": "encrypted-gibberish",
            "encrypted_name": format!("enc-{name_seed}"),
            "name_hash": name_tag(name_seed),
            "mime": "text/plain",
            "size": 1024,
            "chunks": 1,
            "file_id": null,
            "md5": null,
            "sha1": null,
            "sha256": null,
            "blake2b": null,
            "cipher": null,
            "editable": false
        }))
        .to_request();
    let response = test::call_service(app, req).await;
    assert!(
        response.status().is_success(),
        "create {name_seed} failed: {:?}",
        response.status()
    );
    let body: Value = test::read_body_json(response).await;
    entity::Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

fn result_for(body: &Value, id: entity::Uuid) -> &Value {
    body["results"]
        .as_array()
        .expect("results array")
        .iter()
        .find(|r| r["id"].as_str() == Some(&id.to_string()))
        .unwrap_or_else(|| panic!("no result for {id}"))
}

#[actix_web::test]
async fn bulk_disable_enable_delete_users_happy_path() {
    let (app, admin, bob) = setup().await;

    // Disable marks the account and keeps the batch isolated from other ids.
    let (status, body) = bulk_users(&app, &admin.jwt, "disable", &[bob.user_id]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result_for(&body, bob.user_id)["ok"], json!(true));

    let status = get_admin_user(&app, &admin.jwt, bob.user_id).await;
    assert_eq!(status, StatusCode::OK);

    // The admin listing exposes the disabled flag for the UI badge.
    let req = test::TestRequest::get()
        .uri("/api/admin/users")
        .cookie(admin.jwt.clone())
        .to_request();
    let response = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(response).await;
    let entry = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["id"].as_str() == Some(&bob.user_id.to_string()))
        .unwrap();
    assert!(entry["disabled_at"].is_i64(), "disabled_at should be set");

    // Enable clears the flag again.
    let (status, body) = bulk_users(&app, &admin.jwt, "enable", &[bob.user_id]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result_for(&body, bob.user_id)["ok"], json!(true));

    let req = test::TestRequest::get()
        .uri("/api/admin/users")
        .cookie(admin.jwt.clone())
        .to_request();
    let response = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(response).await;
    let entry = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["id"].as_str() == Some(&bob.user_id.to_string()))
        .unwrap();
    assert!(entry["disabled_at"].is_null(), "disabled_at should be cleared");

    // Bulk delete removes the user entirely.
    let (status, body) = bulk_users(&app, &admin.jwt, "delete", &[bob.user_id]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result_for(&body, bob.user_id)["ok"], json!(true));

    let status = get_admin_user(&app, &admin.jwt, bob.user_id).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn bulk_user_error_isolation() {
    let (app, admin, bob) = setup().await;

    // A bogus id between valid ones must not break the batch.
    let bogus = entity::Uuid::new_v4();
    let (status, body) = bulk_users(&app, &admin.jwt, "disable", &[bogus, bob.user_id]).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(result_for(&body, bogus)["ok"], json!(false));
    assert_eq!(
        result_for(&body, bogus)["error"],
        json!("user_not_found"),
        "the bad id carries its own error"
    );

    assert_eq!(result_for(&body, bob.user_id)["ok"], json!(true));

    // Same isolation for delete.
    let (status, body) = bulk_users(&app, &admin.jwt, "delete", &[bob.user_id, bogus]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result_for(&body, bob.user_id)["ok"], json!(true));
    assert_eq!(result_for(&body, bogus)["ok"], json!(false));
}

#[actix_web::test]
async fn bulk_delete_refuses_own_account() {
    let (app, admin, bob) = setup().await;

    // Deleting yourself through a bulk action would lock the operator out;
    // the own id fails per id while the rest of the batch proceeds.
    let (status, body) =
        bulk_users(&app, &admin.jwt, "delete", &[admin.user_id, bob.user_id]).await;
    assert_eq!(status, StatusCode::OK);

    let own = result_for(&body, admin.user_id);
    assert_eq!(own["ok"], json!(false));
    assert_eq!(own["error"], json!("auth::data::staff|forbidden_self"));

    assert_eq!(result_for(&body, bob.user_id)["ok"], json!(true));

    // The admin is still around and functional.
    let status = get_admin_user(&app, &admin.jwt, admin.user_id).await;
    assert_eq!(status, StatusCode::OK);
}

#[actix_web::test]
async fn bulk_users_rejects_oversized_batch() {
    let (app, admin, _bob) = setup().await;

    let ids: Vec<entity::Uuid> = (0..501).map(|_| entity::Uuid::new_v4()).collect();
    let (status, _) = bulk_users(&app, &admin.jwt, "disable", &ids).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn bulk_users_requires_admin() {
    let (app, _admin, bob) = setup().await;

    let (status, body) = bulk_users(&app, &bob.jwt, "disable", &[bob.user_id]).await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED,
        "non-admin must be refused, got {status}: {body}"
    );
}

#[actix_web::test]
async fn bulk_files_delete() {
    let (app, admin, bob) = setup().await;

    let file_id = create_file(&app, &bob.jwt, "bulk-file").await;

    // One good id, one bogus id — the batch survives.
    let bogus = entity::Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri("/api/admin/files/bulk")
        .cookie(admin.jwt.clone())
        .set_json(json!({ "action": "delete", "ids": [file_id, bogus] }))
        .to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;

    assert_eq!(result_for(&body, file_id)["ok"], json!(true));
    assert_eq!(result_for(&body, bogus)["ok"], json!(false));
    assert_eq!(result_for(&body, bogus)["error"], json!("file_not_found"));

    // The file is really gone from the owner's listing.
    let req = test::TestRequest::get()
        .uri("/api/storage")
        .cookie(bob.jwt.clone())
        .to_request();
    let response = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(response).await;
    let ids: Vec<&str> = body["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap())
        .collect();
    assert!(!ids.contains(&file_id.to_string().as_str()));
}

#[actix_web::test]
async fn bulk_files_rejects_non_delete_action() {
    let (app, admin, _bob) = setup().await;

    let req = test::TestRequest::post()
        .uri("/api/admin/files/bulk")
        .cookie(admin.jwt.clone())
        .set_json(json!({ "action": "disable", "ids": [entity::Uuid::new_v4()] }))
        .to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
