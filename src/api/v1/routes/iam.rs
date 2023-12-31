use actix_web::web;

use crate::api::v1::handlers::iam as iam_handlers;
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(iam_handlers::create_user)
        .service(iam_handlers::get_users)
        .service(iam_handlers::get_user)
        .service(iam_handlers::get_user_tokens)
        .service(iam_handlers::update_user)
        .service(iam_handlers::delete_user)
        .service(iam_handlers::create_group)
        .service(iam_handlers::get_group)
        .service(iam_handlers::get_groups)
        .service(iam_handlers::update_group)
        .service(iam_handlers::delete_group)
        .service(iam_handlers::get_group_members)
        .service(iam_handlers::add_group_member)
        .service(iam_handlers::delete_group_member);
}
