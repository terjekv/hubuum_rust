#[cfg(test)]
mod tests {
    use crate::api;
    use crate::config::get_config;
    use crate::db::init_pool;
    use crate::models::user::LoginUser;
    use crate::tests::create_test_user;
    use actix_web::http::header;
    use actix_web::{http::StatusCode, test, web, web::Data, App};
    use diesel::prelude::*;

    const LOGIN_ENDPOINT: &str = "/api/v0/auth/login";
    const LOGOUT_ENDPOINT: &str = "/api/v0/auth/logout";
    const LOGOUT_ALL_ENDPOINT: &str = "/api/v0/auth/logout_all";

    #[actix_web::test]
    async fn test_valid_login() {
        let config = get_config().await;
        let pool = init_pool(&config.database_url, config.db_pool_size);
        let mut conn = pool.get().expect("Failed to get db connection");

        let new_user = create_test_user(&pool).await;

        let app = test::init_service(
            App::new()
                .app_data(Data::new(pool.clone()))
                .configure(api::config),
        )
        .await;

        // Test wrong password
        let login_info = web::Form(LoginUser {
            username: new_user.username.clone(),
            password: "wrongpassword".to_string(),
        });

        // Perform login request
        let resp = test::TestRequest::post()
            .uri(LOGIN_ENDPOINT)
            .set_json(&login_info)
            .send_request(&app)
            .await;

        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "{:?}",
            test::read_body(resp).await
        );

        let login_info_ok = web::Form(LoginUser {
            username: new_user.username.clone(),
            password: "testpassword".to_string(),
        });

        // Perform login request
        let resp = test::TestRequest::post()
            .uri(LOGIN_ENDPOINT)
            .set_json(&login_info_ok)
            .send_request(&app)
            .await;

        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "{:?}",
            test::read_body(resp).await
        );

        // Check Content Type
        let content_type_header = resp
            .headers()
            .get("Content-Type")
            .unwrap()
            .to_str()
            .unwrap();

        assert!(
            content_type_header.contains("application/json"),
            "Content type is not JSON"
        );

        let body = test::read_body(resp).await;
        let body_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(
            body_json.get("token").is_some(),
            "Response does not contain token"
        );

        let token_value = body_json
            .get("token")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        // Verify token in database and that it belongs to the user
        use crate::models::token::UserToken;
        use crate::schema::tokens::dsl::*;
        let token_exists = tokens
            .filter(token.eq(&token_value))
            .filter(user_id.eq(new_user.id))
            .first::<UserToken>(&mut conn)
            .is_ok();

        assert!(token_exists, "Token not found in database");
    }

    #[actix_web::test]
    async fn test_invalid_login_credentials() {
        let config = get_config().await;
        let pool = init_pool(&config.database_url, config.db_pool_size);
        let app = test::init_service(
            App::new()
                .app_data(Data::new(pool.clone()))
                .configure(api::config),
        )
        .await;

        let login_info = web::Form(LoginUser {
            username: "nosuchuser".to_string(),
            password: "nosuchpassword".to_string(),
        });

        // Perform login request
        let resp = test::TestRequest::post()
            .uri(LOGIN_ENDPOINT)
            .set_json(&login_info)
            .send_request(&app)
            .await;

        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "{:?}",
            test::read_body(resp).await
        );
    }

    #[actix_web::test]
    async fn test_invalid_login_parameters() {
        let config = get_config().await;
        let pool = init_pool(&config.database_url, config.db_pool_size);

        let app = test::init_service(
            App::new()
                .app_data(Data::new(pool.clone()))
                .configure(api::config),
        )
        .await;

        #[derive(Debug, serde::Deserialize, serde::Serialize)]
        struct NoPassword {
            username: String,
        }

        #[derive(Debug, serde::Deserialize, serde::Serialize)]
        struct NoUsername {
            password: String,
        }

        // Perform login request with username but no password element
        let login_info_no_password = web::Form(NoPassword {
            username: "testuser".to_string(),
        });

        let resp = test::TestRequest::post()
            .uri(LOGIN_ENDPOINT)
            .set_json(&login_info_no_password)
            .send_request(&app)
            .await;

        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "{:?}",
            test::read_body(resp).await
        );

        let login_info_no_username = web::Form(NoUsername {
            password: "password".to_string(),
        });

        let resp = test::TestRequest::post()
            .uri(LOGIN_ENDPOINT)
            .set_json(&login_info_no_username)
            .send_request(&app)
            .await;

        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "{:?}",
            test::read_body(resp).await
        );
    }

    #[actix_web::test]
    async fn test_logout_single_token() {
        let config = get_config().await;
        let pool = init_pool(&config.database_url, config.db_pool_size);

        let new_user = create_test_user(&pool).await;

        let token_string = match { new_user.create_token(&pool).await } {
            Ok(ret_token) => ret_token.get_token(),
            Err(e) => panic!("Failed to add token to user: {:?}", e),
        };

        let app = test::init_service(
            App::new()
                .app_data(Data::new(pool.clone()))
                .configure(api::config),
        )
        .await;

        let user_tokens = new_user.get_tokens(&pool).await.unwrap();
        assert_eq!(user_tokens.len(), 1, "Token count mismatch");

        let resp_without_token = test::TestRequest::get()
            .uri(LOGOUT_ENDPOINT)
            .send_request(&app)
            .await;

        assert_eq!(
            resp_without_token.status(),
            StatusCode::UNAUTHORIZED,
            "{:?}",
            test::read_body(resp_without_token).await
        );

        let resp_with_broken_token = test::TestRequest::get()
            .insert_header((header::AUTHORIZATION, "nope".to_string()))
            .uri(LOGOUT_ENDPOINT)
            .send_request(&app)
            .await;

        assert_eq!(
            resp_with_broken_token.status(),
            StatusCode::UNAUTHORIZED,
            "{:?}",
            test::read_body(resp_with_broken_token).await
        );

        let resp = test::TestRequest::get()
            .insert_header((header::AUTHORIZATION, format!("Bearer {}", token_string)))
            .uri(LOGOUT_ENDPOINT)
            .send_request(&app)
            .await;

        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "{:?}",
            test::read_body(resp).await
        );

        // Verify token is gone from database
        let user_tokens = new_user.get_tokens(&pool).await.unwrap();
        assert_eq!(user_tokens.len(), 0, "User still has tokens");
    }

    #[actix_web::test]
    async fn test_logout_all_tokens() {
        let config = get_config().await;
        let pool = init_pool(&config.database_url, config.db_pool_size);

        let new_user = create_test_user(&pool).await;

        let token_string = match { new_user.create_token(&pool).await } {
            Ok(ret_token) => ret_token.get_token(),
            Err(e) => panic!("Failed to add token to user: {:?}", e),
        };

        let _ = match { new_user.create_token(&pool).await } {
            Ok(ret_token) => ret_token.get_token(),
            Err(e) => panic!("Failed to add token to user: {:?}", e),
        };

        // Verify that we have two tokens for the user
        let user_tokens = new_user.get_tokens(&pool).await.unwrap();
        assert_eq!(user_tokens.len(), 2, "User has wrong number of tokens");

        let app = test::init_service(
            App::new()
                .app_data(Data::new(pool.clone()))
                .configure(api::config),
        )
        .await;

        // Try removing tokens without authorization
        let resp_without_token = test::TestRequest::get()
            .uri(LOGOUT_ALL_ENDPOINT)
            .send_request(&app)
            .await;

        assert_eq!(
            resp_without_token.status(),
            StatusCode::UNAUTHORIZED,
            "{:?}",
            test::read_body(resp_without_token).await
        );
        let user_tokens = new_user.get_tokens(&pool).await.unwrap();
        assert_eq!(user_tokens.len(), 2, "User has wrong number of tokens");

        // Try removing tokens with broken authorization
        let resp_with_broken_token = test::TestRequest::get()
            .insert_header((header::AUTHORIZATION, "nope".to_string()))
            .uri(LOGOUT_ALL_ENDPOINT)
            .send_request(&app)
            .await;

        assert_eq!(
            resp_with_broken_token.status(),
            StatusCode::UNAUTHORIZED,
            "{:?}",
            test::read_body(resp_with_broken_token).await
        );
        let user_tokens = new_user.get_tokens(&pool).await.unwrap();
        assert_eq!(user_tokens.len(), 2, "User has wrong number of tokens");

        // Remove tokens with valid authorization
        let resp = test::TestRequest::get()
            .insert_header((header::AUTHORIZATION, format!("Bearer {}", token_string)))
            .uri(LOGOUT_ALL_ENDPOINT)
            .send_request(&app)
            .await;

        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "{:?}",
            test::read_body(resp).await
        );

        let user_tokens = new_user.get_tokens(&pool).await.unwrap();
        assert_eq!(user_tokens.len(), 0, "User still has tokens");
    }
}
