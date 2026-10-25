//! Audit log integration tests: login success/failure and admin actions
//! append rows to `audit_logs`, and `GET /api/admin/audit` serves them
//! paginated, newest-first, with user / action-prefix / date-range filters.

#[path = "./helpers.rs"]
mod helpers;

use actix_web::{http::StatusCode, test};
use entity::{
    audit_log, users, ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait,
    QueryFilter,
};
use jiamiyunpan::server;
use serde_json::{json, Value};

use crate::helpers::LEGACY_PASSWORD;

/// Login through the real credentials route, returning the status and the
/// JWT cookie (on success).
async fn login(
    app: &impl helpers::TestApp,
    email: &str,
    password: &str,
) -> (StatusCode, Option<actix_web::cookie::Cookie<'static>>) {
    let req = test::TestRequest::post()
        .uri("/api/auth/login")
        .set_json(json!({ "email": email, "password": password }))
        .to_request();
    let resp = test::call_service(app, req).await;

    let status = resp.status();
    let (jwt, _) = helpers::extract_cookies(resp.headers());
    (status, jwt)
}

/// GET /api/admin/audit with the given query string.
async fn get_audit(
    app: &impl helpers::TestApp,
    jwt: &actix_web::cookie::Cookie<'static>,
    query: &str,
) -> Value {
    let req = test::TestRequest::get()
        .uri(&format!("/api/admin/audit{query}"))
        .cookie(jwt.clone())
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    test::read_body_json(resp).await
}

#[actix_web::test]
async fn login_and_admin_actions_are_recorded() {
    let context = context::Context::mock_sqlite().await;
    let app = test::init_service(server::app(context.clone())).await;

    // First seeded account becomes the instance admin, second is a target user.
    helpers::seed_legacy_user(&context.db, "audit-admin@example.com").await;
    helpers::seed_legacy_user(&context.db, "audit-bob@example.com").await;

    let admin = users::Entity::find()
        .filter(users::Column::Email.eq("audit-admin@example.com"))
        .one(&context.db)
        .await
        .unwrap()
        .unwrap();
    let bob = users::Entity::find()
        .filter(users::Column::Email.eq("audit-bob@example.com"))
        .one(&context.db)
        .await
        .unwrap()
        .unwrap();

    // Failed login — recorded against the attempted identity, no user id.
    let (status, _) = login(&app, "audit-bob@example.com", "definitely-wrong").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Successful admin login — recorded against the authenticated user.
    let (status, jwt) = login(&app, "audit-admin@example.com", LEGACY_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let jwt = jwt.expect("admin login response missing JWT cookie");

    // Admin changes bob's role.
    let req = test::TestRequest::put()
        .uri(&format!("/api/admin/users/{}", bob.id))
        .cookie(jwt.clone())
        .set_json(json!({ "role": "admin" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Admin deletes bob.
    let req = test::TestRequest::delete()
        .uri(&format!("/api/admin/users/{}", bob.id))
        .cookie(jwt.clone())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let mut rows = audit_log::Entity::find()
        .all(&context.db)
        .await
        .unwrap();
    rows.sort_by_key(|r| r.id);

    let actions: Vec<&str> = rows.iter().map(|r| r.action.as_str()).collect();
    assert_eq!(
        actions,
        vec![
            "login.failure",
            "login.success",
            "admin.user.update",
            "admin.user.delete",
        ]
    );

    let failure = &rows[0];
    assert_eq!(failure.user_id, None);
    assert_eq!(failure.target.as_deref(), Some("email:audit-bob@example.com"));
    assert!(failure.ip.is_some());
    assert!(failure.metadata.is_some());

    let success = &rows[1];
    assert_eq!(success.user_id, Some(admin.id));
    assert!(success.ip.is_some());

    let update = &rows[2];
    assert_eq!(update.user_id, Some(admin.id));
    assert_eq!(
        update.target.as_deref(),
        Some(format!("user:{}", bob.id).as_str())
    );
    let metadata: Value =
        serde_json::from_str(update.metadata.as_deref().unwrap()).unwrap();
    assert_eq!(metadata["role"], "admin");

    let delete = &rows[3];
    assert_eq!(delete.user_id, Some(admin.id));
    assert_eq!(
        delete.target.as_deref(),
        Some(format!("user:{}", bob.id).as_str())
    );
}

#[actix_web::test]
async fn admin_audit_endpoint_paginates_and_filters() {
    let context = context::Context::mock_sqlite().await;
    let app = test::init_service(server::app(context.clone())).await;

    helpers::seed_legacy_user(&context.db, "audit-admin2@example.com").await;

    let (status, jwt) = login(&app, "audit-admin2@example.com", LEGACY_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let jwt = jwt.expect("admin login response missing JWT cookie");

    let admin = users::Entity::find()
        .filter(users::Column::Email.eq("audit-admin2@example.com"))
        .one(&context.db)
        .await
        .unwrap()
        .unwrap();

    // Five "recent" rows through the real helper (all stamped with `now`).
    for i in 0..5 {
        let action = if i % 2 == 0 {
            "login.success"
        } else {
            "admin.user.delete"
        };
        audit_log::record(&context.db, Some(admin.id), action, None, None, None, None).await;
    }
    // One stale row (ancient `created_at`) and one unattributed row.
    audit_log::ActiveModel {
        action: ActiveValue::Set("admin.settings.update".to_string()),
        created_at: ActiveValue::Set(1000),
        ..Default::default()
    }
    .insert(&context.db)
    .await
    .unwrap();
    audit_log::record(&context.db, None, "login.failure", None, None, None, None).await;

    // Newest first: the unattributed failure (seeded last) leads the page.
    // 8 rows total: the real admin login above already recorded its own
    // `login.success`, plus 5 helper rows, 1 stale row, 1 unattributed row.
    let body = get_audit(&app, &jwt, "").await;
    assert_eq!(body["total"], 8);
    assert_eq!(body["data"].as_array().unwrap().len(), 8);
    assert_eq!(body["data"][0]["action"], "login.failure");
    assert_eq!(body["data"][7]["action"], "admin.settings.update");

    // Pagination: a limited page reports the full total.
    let body = get_audit(&app, &jwt, "?limit=2&offset=0").await;
    assert_eq!(body["total"], 8);
    assert_eq!(body["data"].as_array().unwrap().len(), 2);

    let body = get_audit(&app, &jwt, "?limit=2&offset=6").await;
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"][1]["action"], "admin.settings.update");

    // Action-prefix filter: `login` matches success and failure, not admin.
    let body = get_audit(&app, &jwt, "?action=login").await;
    assert_eq!(body["total"], 5);
    for row in body["data"].as_array().unwrap() {
        assert!(row["action"]
            .as_str()
            .unwrap()
            .starts_with("login"));
    }

    let body = get_audit(&app, &jwt, "?action=admin.user").await;
    assert_eq!(body["total"], 2);

    // User filter excludes the unattributed rows.
    let body = get_audit(&app, &jwt, &format!("?user_id={}", admin.id)).await;
    assert_eq!(body["total"], 6);

    // Date range: only the ancient settings-update row falls inside.
    let body = get_audit(&app, &jwt, "?created_from=0&created_to=2000").await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["data"][0]["action"], "admin.settings.update");

    let now = chrono::Utc::now().timestamp();
    let body = get_audit(
        &app,
        &jwt,
        &format!("?created_from={}&created_to={}", now - 60, now + 60),
    )
    .await;
    assert_eq!(body["total"], 7);

    // Non-admins must not read the trail.
    let req = test::TestRequest::get()
        .uri("/api/admin/audit")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(!resp.status().is_success());

    // Sanity: the seeded trail matches the direct count.
    let count = audit_log::Entity::find().count(&context.db).await.unwrap();
    assert_eq!(count, 8);
}
