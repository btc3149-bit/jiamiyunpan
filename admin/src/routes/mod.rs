pub mod audit;
pub mod files;
pub mod invitations;
pub mod sessions;
pub mod settings;
pub mod users;

pub fn configure(cfg: &mut actix_web::web::ServiceConfig) {
    cfg.service(audit::index)
        .service(files::index)
        .service(files::bulk)
        .service(invitations::create)
        .service(invitations::expire)
        .service(invitations::index)
        .service(sessions::index)
        .service(sessions::kill)
        .service(sessions::kill_for_user)
        .service(users::get)
        .service(users::bulk)
        .service(users::index)
        .service(users::update)
        .service(users::remove)
        .service(settings::index)
        .service(settings::update)
        .service(settings::test_email)
        .service(users::remove_tfa)
        .service(users::verify_email);
}
