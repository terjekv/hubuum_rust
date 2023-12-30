// src/db/connection.rs

use diesel::r2d2::ConnectionManager;
use diesel::r2d2::Pool;
use diesel::PgConnection;
use tracing::debug;

use crate::db::DatabaseOps;
use crate::errors::ApiError;
use crate::extractors::BearerToken;
use crate::utilities::db::DatabaseUrlComponents;

pub type DbPool = Pool<ConnectionManager<PgConnection>>;

impl DatabaseOps for DbPool {
    fn get_valid_token(&self, token: &str) -> Result<BearerToken, ApiError> {
        use crate::schema::tokens::dsl::{expires, token as token_column, tokens};
        use chrono::prelude::Utc;
        use diesel::prelude::{ExpressionMethods, QueryDsl, RunQueryDsl};

        let mut conn = self.get().expect("couldn't get db connection from pool");

        let now = Utc::now().naive_utc();

        let token_result = tokens
            .filter(token_column.eq(token))
            .filter(expires.gt(now))
            .first::<crate::models::token::Token>(&mut conn);

        match token_result {
            Ok(token_data) => Ok(BearerToken {
                token: token_data.token,
                user_id: token_data.user_id,
            }),
            Err(e) => {
                debug!(
                    message = "Token validation failed",
                    token = token,
                    error = e.to_string()
                );
                return Err(ApiError::Unauthorized(
                    "Token validation failed".to_string(),
                ));
            }
        }
    }
}

pub fn init_pool(database_url: &str, max_size: u32) -> DbPool {
    let database_url_components = DatabaseUrlComponents::new(database_url);

    match database_url_components {
        Ok(components) => {
            debug!(
                message = "Database URL parsed.",
                vendor = components.vendor,
                username = components.username,
                host = components.host,
                port = components.port,
                database = components.database,
            );
        }
        Err(err) => panic!("{}", err),
    }

    let manager = ConnectionManager::<PgConnection>::new(database_url);
    let pool = Pool::builder()
        .max_size(max_size)
        .build(manager)
        .expect("Failed to create pool");

    return pool;
}

#[cfg(test)]
mod tests {
    use crate::utilities::test::test_database_url;

    #[test]
    fn test_init_pool() {
        let database_url = test_database_url();
        let pool = super::init_pool(&database_url, 5);
        assert_eq!(pool.max_size(), 10);
    }
}
