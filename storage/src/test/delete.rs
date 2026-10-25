use crate::{mock::create_file, repository::Repository};
use context::Context;
use entity::EntityTrait;

#[actix_web::test]
async fn create_recursive_mess_to_test_trash_many() {
    let context = Context::mock_sqlite().await;
    let repository = Repository::new(&context.db);
    let user = entity::mock::create_user(&context.db, "first@test.com", None).await;

    let mut manual = vec![];

    let dir = create_file(&context, &user, "root.dir", None, Some("dir"))
        .await
        .unwrap();
    let dir_id = dir.id;
    manual.push(dir);

    let file = create_file(
        &context,
        &user,
        "root.json1",
        None,
        Some("application/json"),
    )
    .await
    .unwrap();
    manual.push(file);

    let file = create_file(
        &context,
        &user,
        "root.json2",
        None,
        Some("application/json"),
    )
    .await
    .unwrap();
    let _file2_id = file.id;
    manual.push(file);

    let file = create_file(
        &context,
        &user,
        "root.json3",
        None,
        Some("application/json"),
    )
    .await
    .unwrap();
    manual.push(file);

    let file = create_file(
        &context,
        &user,
        "root.json4",
        None,
        Some("application/json"),
    )
    .await
    .unwrap();
    manual.push(file);

    let dir = create_file(&context, &user, "root.dir.dir2", Some(dir_id), Some("dir"))
        .await
        .unwrap();
    let dir2_id = dir.id;
    manual.push(dir);

    let dir3 = create_file(
        &context,
        &user,
        "root.dir.dir2.dir3",
        Some(dir2_id),
        Some("dir"),
    )
    .await
    .unwrap();
    manual.push(dir3);

    let ids = manual.iter().map(|f| f.id).collect::<Vec<_>>();

    // Soft delete: every owned row in the trashed subtrees gets one
    // `trashed_files` marker sharing the same deletion timestamp; the file
    // rows themselves stay.
    let trashed_files = repository
        .manage(user.id)
        .trash_many(ids.clone())
        .await
        .unwrap();

    for file in manual.iter() {
        assert!(file.is_owner);
    }

    assert_ne!(trashed_files.len(), 0);

    assert_eq!(manual.len(), trashed_files.len());

    let deleted_at = entity::trash::Entity::find_by_id(dir_id)
        .one(&context.db)
        .await
        .unwrap()
        .expect("dir must carry a trash marker")
        .deleted_at;

    let markers = entity::trash::Entity::find()
        .all(&context.db)
        .await
        .unwrap();
    assert_eq!(markers.len(), manual.len());
    assert!(markers
        .iter()
        .all(|m| m.deleted_at == deleted_at));

    // The rows themselves are untouched — only the retention sweep or a
    // permanent delete removes them.
    for file in manual.iter() {
        let row = entity::files::Entity::find_by_id(file.id)
            .one(&context.db)
            .await
            .unwrap();
        assert!(row.is_some());
    }
}
