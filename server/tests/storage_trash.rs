//! Trash (soft delete) integration tests: the DELETE route now moves files
//! into the trash instead of purging, restore brings them back (re-rooting
//! them when the parent is gone, refusing/applying rename on collision),
//! permanent delete and empty-trash really purge, trashed bytes leave the
//! quota, and the retention sweep purges by an injected clock.

#[path = "./helpers.rs"]
mod helpers;

use actix_web::{
    http::{Method, StatusCode},
    test,
};
use jiamiyunpan::server;
use serde_json::{json, Value};

use crate::helpers::{name_tag, seed_owned_files};

/// Create a file (or folder with `mime: "dir"`) through the real route.
async fn create_file(
    app: &impl helpers::TestApp,
    jwt: &actix_web::cookie::Cookie<'static>,
    name_seed: &str,
    mime: &str,
    parent: Option<String>,
) -> Value {
    // Directories reject size/chunks at validation; files require them.
    let mut body = json!({
        "encrypted_key": "encrypted-gibberish",
        "encrypted_name": format!("enc-{name_seed}"),
        "name_hash": name_tag(name_seed),
        "mime": mime,
        "file_id": parent,
        "md5": null,
        "sha1": null,
        "sha256": null,
        "blake2b": null,
        "cipher": null,
        "editable": false
    });

    if mime != "dir" {
        body["size"] = json!(1024);
        body["chunks"] = json!(1);
    }

    let req = test::TestRequest::post()
        .uri("/api/storage")
        .cookie(jwt.clone())
        .set_json(&body)
        .to_request();

    let response = test::call_service(app, req).await;
    assert!(
        response.status().is_success(),
        "create {name_seed} failed: {:?}",
        response.status()
    );

    test::read_body_json(response).await
}

async fn trash_list(
    app: &impl helpers::TestApp,
    jwt: &actix_web::cookie::Cookie<'static>,
) -> Value {
    let req = test::TestRequest::get()
        .uri("/api/storage/trash")
        .cookie(jwt.clone())
        .to_request();
    let response = test::call_service(app, req).await;
    assert_eq!(response.status(), StatusCode::OK);
    test::read_body_json(response).await
}

async fn used_space(
    app: &impl helpers::TestApp,
    jwt: &actix_web::cookie::Cookie<'static>,
) -> i64 {
    let req = test::TestRequest::post()
        .uri("/api/storage/stats")
        .cookie(jwt.clone())
        .set_json(json!({}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(app, req).await).await;
    body["used_space"].as_i64().unwrap()
}

async fn listing_ids(
    app: &impl helpers::TestApp,
    jwt: &actix_web::cookie::Cookie<'static>,
) -> Vec<String> {
    let req = test::TestRequest::with_uri("/api/storage")
        .method(Method::GET)
        .cookie(jwt.clone())
        .to_request();
    let response = test::call_service(app, req).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = test::read_body_json(response).await;
    body["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap().to_string())
        .collect()
}

#[actix_web::test]
async fn soft_delete_hides_from_listings_and_trash_restores() {
    let context = context::Context::mock_with_data_dir(Some("../data/test".to_string())).await;
    let app = test::init_service(server::app(context.clone())).await;

    let jwt = helpers::register_curve25519(&app, "trash-basic@doe.com").await.jwt;

    let file = create_file(&app, &jwt, "basic-file", "text/plain", None).await;
    let file_id = file["id"].as_str().unwrap().to_string();

    // Live before, in the listing.
    assert!(listing_ids(&app, &jwt).await.contains(&file_id));

    // Owner delete → soft delete, 204, nothing purged.
    let req = test::TestRequest::delete()
        .uri(format!("/api/storage/{file_id}").as_str())
        .cookie(jwt.clone())
        .to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Hidden from the listing and from metadata.
    assert!(!listing_ids(&app, &jwt).await.contains(&file_id));

    let req = test::TestRequest::get()
        .uri(format!("/api/storage/{file_id}/metadata").as_str())
        .cookie(jwt.clone())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );

    // The trash lists it with a deletion timestamp.
    let trash = trash_list(&app, &jwt).await;
    let items = trash["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["file"]["id"].as_str().unwrap(), file_id);
    assert!(items[0]["deleted_at"].as_i64().unwrap() > 0);

    // Restore → back in the listing.
    let req = test::TestRequest::post()
        .uri(format!("/api/storage/trash/{file_id}/restore").as_str())
        .cookie(jwt.clone())
        .set_json(json!({}))
        .to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(response.status(), StatusCode::OK);
    let restored: Value = test::read_body_json(response).await;
    assert_eq!(restored["moved_to_root"], json!(false));

    assert!(listing_ids(&app, &jwt).await.contains(&file_id));

    let trash = trash_list(&app, &jwt).await;
    assert_eq!(trash["items"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn non_owner_cannot_touch_trash() {
    let context = context::Context::mock_with_data_dir(Some("../data/test".to_string())).await;
    let app = test::init_service(server::app(context.clone())).await;

    let jwt = helpers::register_curve25519(&app, "trash-owner@doe.com").await.jwt;
    let stranger = helpers::register_curve25519(&app, "trash-stranger@doe.com").await;

    let file = create_file(&app, &jwt, "forbidden-file", "text/plain", None).await;
    let file_id = file["id"].as_str().unwrap().to_string();

    let req = test::TestRequest::delete()
        .uri(format!("/api/storage/{file_id}").as_str())
        .cookie(jwt.clone())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    // The trash listing is per-caller — the stranger sees their own (empty)
    // trash, never the owner's.
    let trash = trash_list(&app, &stranger.jwt).await;
    assert_eq!(trash["items"].as_array().unwrap().len(), 0);

    // Restore / purge of someone else's file with no user_files row →
    // forbidden. This codebase maps `Error::Forbidden` to 401 on the wire.
    for (method, uri) in [
        (&Method::POST, format!("/api/storage/trash/{file_id}/restore").as_str()),
        (&Method::DELETE, format!("/api/storage/trash/{file_id}").as_str()),
    ] {
        let req = test::TestRequest::with_uri(uri)
            .method(method.clone())
            .cookie(stranger.jwt.clone())
            .set_json(json!({}))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::UNAUTHORIZED,
            "{} {}",
            method,
            uri
        );
    }

    // Empty-trash is scoped to the caller's own rows: for the stranger it
    // is a legitimate no-op, not a way to touch the owner's entries.
    let req = test::TestRequest::post()
        .uri("/api/storage/trash/empty")
        .cookie(stranger.jwt.clone())
        .set_json(json!({}))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );
}

#[actix_web::test]
async fn restore_re_roots_when_parent_is_trashed() {
    let context = context::Context::mock_with_data_dir(Some("../data/test".to_string())).await;
    let app = test::init_service(server::app(context.clone())).await;

    let jwt = helpers::register_curve25519(&app, "trash-parent@doe.com").await.jwt;

    let dir = create_file(&app, &jwt, "parent-dir", "dir", None).await;
    let dir_id = dir["id"].as_str().unwrap().to_string();

    let file = create_file(&app, &jwt, "nested-file", "text/plain", Some(dir_id.clone())).await;
    let file_id = file["id"].as_str().unwrap().to_string();

    // Trash the folder — the file is stamped by the same operation.
    let req = test::TestRequest::delete()
        .uri(format!("/api/storage/{dir_id}").as_str())
        .cookie(jwt.clone())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    // Only the folder is a trash root; the nested file hides behind it.
    let trash = trash_list(&app, &jwt).await;
    let roots = trash["items"].as_array().unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0]["file"]["id"].as_str().unwrap(), dir_id);

    // Restoring the nested file directly: its parent is trashed, so it
    // re-attaches to the drive root — and the folder stays trashed.
    let req = test::TestRequest::post()
        .uri(format!("/api/storage/trash/{file_id}/restore").as_str())
        .cookie(jwt.clone())
        .set_json(json!({}))
        .to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(response.status(), StatusCode::OK);
    let restored: Value = test::read_body_json(response).await;
    assert_eq!(restored["moved_to_root"], json!(true));

    let ids = listing_ids(&app, &jwt).await;
    assert!(ids.contains(&file_id));
    assert!(!ids.contains(&dir_id));

    let trash = trash_list(&app, &jwt).await;
    let roots = trash["items"].as_array().unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0]["file"]["id"].as_str().unwrap(), dir_id);
}

#[actix_web::test]
async fn restore_name_conflict_conflicts_then_renames() {
    let context = context::Context::mock_with_data_dir(Some("../data/test".to_string())).await;
    let app = test::init_service(server::app(context.clone())).await;

    let jwt = helpers::register_curve25519(&app, "trash-conflict@doe.com").await.jwt;

    let file = create_file(&app, &jwt, "clash-file", "text/plain", None).await;
    let file_id = file["id"].as_str().unwrap().to_string();

    let req = test::TestRequest::delete()
        .uri(format!("/api/storage/{file_id}").as_str())
        .cookie(jwt.clone())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    // A live sibling now occupies the name.
    let _ = create_file(&app, &jwt, "clash-file", "text/plain", None).await;

    // Plain restore → 409 with the conflict marker.
    let req = test::TestRequest::post()
        .uri(format!("/api/storage/trash/{file_id}/restore").as_str())
        .cookie(jwt.clone())
        .set_json(json!({}))
        .to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: Value = test::read_body_json(response).await;
    assert!(body["message"]
        .as_str()
        .unwrap_or_default()
        .contains("trash_restore_name_conflict"));

    // Retry with a re-encrypted suffixed name → applies and restores.
    let req = test::TestRequest::post()
        .uri(format!("/api/storage/trash/{file_id}/restore").as_str())
        .cookie(jwt.clone())
        .set_json(json!({
            "encrypted_name": "enc-clash-file (restored)",
            "name_hash": name_tag("clash-file (restored)"),
            "search_tokens_root": [],
            "search_tokens_file": []
        }))
        .to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(response.status(), StatusCode::OK);
    let restored: Value = test::read_body_json(response).await;
    assert_eq!(restored["renamed"], json!(true));

    assert!(listing_ids(&app, &jwt).await.contains(&file_id));
}

#[actix_web::test]
async fn permanent_delete_and_empty_purge() {
    let context = context::Context::mock_with_data_dir(Some("../data/test".to_string())).await;
    let app = test::init_service(server::app(context.clone())).await;

    let jwt = helpers::register_curve25519(&app, "trash-purge@doe.com").await.jwt;

    let first = create_file(&app, &jwt, "purge-first", "text/plain", None).await;
    let second = create_file(&app, &jwt, "purge-second", "text/plain", None).await;
    let first_id = first["id"].as_str().unwrap().to_string();
    let second_id = second["id"].as_str().unwrap().to_string();

    for id in [&first_id, &second_id] {
        let req = test::TestRequest::delete()
            .uri(format!("/api/storage/{id}").as_str())
            .cookie(jwt.clone())
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::NO_CONTENT
        );
    }

    // Permanently delete one entry.
    let req = test::TestRequest::delete()
        .uri(format!("/api/storage/trash/{first_id}").as_str())
        .cookie(jwt.clone())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let trash = trash_list(&app, &jwt).await;
    let items = trash["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["file"]["id"].as_str().unwrap(), second_id);

    // It is really gone: no row, no metadata.
    let req = test::TestRequest::get()
        .uri(format!("/api/storage/{first_id}/metadata").as_str())
        .cookie(jwt.clone())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );

    // Empty the rest.
    let req = test::TestRequest::post()
        .uri("/api/storage/trash/empty")
        .cookie(jwt.clone())
        .set_json(json!({}))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let trash = trash_list(&app, &jwt).await;
    assert_eq!(trash["items"].as_array().unwrap().len(), 0);

    let req = test::TestRequest::get()
        .uri(format!("/api/storage/{second_id}/metadata").as_str())
        .cookie(jwt.clone())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[actix_web::test]
async fn trashed_files_leave_the_quota() {
    let context = context::Context::mock_with_data_dir(Some("../data/test".to_string())).await;
    let app = test::init_service(server::app(context.clone())).await;

    let jwt = helpers::register_curve25519(&app, "trash-quota@doe.com").await.jwt;

    let file = create_file(&app, &jwt, "quota-file", "text/plain", None).await;
    let file_id = file["id"].as_str().unwrap().to_string();

    assert_eq!(used_space(&app, &jwt).await, 1024);

    let req = test::TestRequest::delete()
        .uri(format!("/api/storage/{file_id}").as_str())
        .cookie(jwt.clone())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    // Trashed bytes are freed immediately; restore charges them back.
    assert_eq!(used_space(&app, &jwt).await, 0);

    let req = test::TestRequest::post()
        .uri(format!("/api/storage/trash/{file_id}/restore").as_str())
        .cookie(jwt.clone())
        .set_json(json!({}))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::OK);

    assert_eq!(used_space(&app, &jwt).await, 1024);
}

#[actix_web::test]
async fn retention_sweep_purges_expired_only() {
    let context = context::Context::mock_with_data_dir(Some("../data/test".to_string())).await;
    let app = test::init_service(server::app(context.clone())).await;

    let registered = helpers::register_curve25519(&app, "trash-sweep@doe.com").await;
    let (jwt, user_id) = (registered.jwt, registered.user_id);

    // Two owned rows seeded directly: one freshly trashed, one trashed
    // 40 days ago. Retention is 30 days — only the old one may die.
    let ids = seed_owned_files(&context.db, user_id, 2).await;
    let (fresh, expired) = (ids[0], ids[1]);
    let now = 1_800_000_000i64;

    use entity::{trash, ActiveModelTrait, ActiveValue, EntityTrait};
    for (id, deleted_at) in [(fresh, now), (expired, now - 40 * 86_400)] {
        trash::ActiveModel {
            file_id: ActiveValue::Set(id),
            deleted_at: ActiveValue::Set(deleted_at),
        }
        .insert(&context.db)
        .await
        .unwrap();
    }

    use fs::prelude::Fs;
    let fs = Fs::new(&context.config);
    let purged = storage::cleanup::purge_expired(&context.db, &fs, now, 30)
        .await
        .unwrap();
    assert_eq!(purged, 1);

    let gone = entity::files::Entity::find_by_id(expired)
        .one(&context.db)
        .await
        .unwrap();
    assert!(gone.is_none(), "expired file must be purged");

    let alive = entity::files::Entity::find_by_id(fresh)
        .one(&context.db)
        .await
        .unwrap();
    assert!(alive.is_some(), "fresh trash must survive the sweep");

    // The trash routes reflect the same state: only the fresh entry left.
    let trash = trash_list(&app, &jwt).await;
    let items = trash["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["file"]["id"].as_str().unwrap(), fresh.to_string());
}
