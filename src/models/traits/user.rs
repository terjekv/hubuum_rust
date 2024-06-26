use diesel::sql_types::Integer;
use diesel::{pg::Pg, ExpressionMethods, JoinOnDsl, QueryDsl, RunQueryDsl, Table};

use crate::api::v1::handlers::namespaces;
use crate::models::search::SearchOperator;
use crate::models::{
    class, permissions, Group, HubuumClass, HubuumObject, Namespace, Permission, Permissions, User,
    UserID,
};

use crate::schema::{hubuumclass, hubuumobject};
use crate::traits::{ClassAccessors, NamespaceAccessors, SelfAccessors};

use crate::db::DbPool;
use crate::errors::ApiError;
use crate::utilities::extensions::CustomStringExtensions;

use futures::future::try_join_all;
use tracing::debug;

use crate::models::search::{ParsedQueryParam, QueryParamsExt};

use crate::trace_query;

pub trait Search: SelfAccessors<User> + GroupAccessors + UserNamespaceAccessors {
    async fn search_classes(
        &self,
        pool: &DbPool,
        query_params: Vec<ParsedQueryParam>,
    ) -> Result<Vec<HubuumClass>, ApiError> {
        use crate::models::PermissionFilter;
        use crate::schema::hubuumclass::dsl::{
            hubuumclass, id as hubuum_class_id, namespace_id as hubuum_classes_nid,
        };
        use crate::schema::permissions::dsl::*;

        debug!(
            message = "Searching classes",
            stage = "Starting",
            user_id = self.id(),
            query_params = ?query_params
        );

        let mut conn = pool.get()?;
        let group_id_subquery = self.group_ids_subquery();

        // Get all namespace IDs that the user has read permissions on, and if we have a list of selected namespaces, filter on those.
        let namespace_ids: Vec<i32> = self
            .namespaces_read(pool)
            .await?
            .into_iter()
            .map(|n| n.id)
            .collect();

        debug!(
            message = "Searching classes",
            stage = "Namespace IDs",
            user_id = self.id(),
            namespace_ids = ?namespace_ids
        );

        let mut base_query = permissions
            .into_boxed()
            .filter(group_id.eq_any(group_id_subquery));

        // Handle permissions
        for perm in query_params.permissions()? {
            base_query = perm.create_boxed_filter(base_query, true);
        }

        let mut base_query =
            base_query.inner_join(hubuumclass.on(hubuum_classes_nid.eq_any(namespace_ids)));

        let json_schema_queries = query_params.json_schemas()?;
        if !json_schema_queries.is_empty() {
            debug!(
                message = "Searching classes",
                stage = "JSON Schema",
                user_id = self.id(),
                query_params = ?json_schema_queries
            );

            let json_schema_integers = self.json_schema_subquery(pool, json_schema_queries)?;

            if json_schema_integers.is_empty() {
                debug!(
                    message = "Searching classes",
                    stage = "JSON Schema",
                    user_id = self.id(),
                    result = "No class IDs found, returning empty result"
                );
                return Ok(vec![]);
            }

            debug!(
                message = "Searching classes",
                stage = "JSON Schema",
                user_id = self.id(),
                result = "Found class IDs",
                class_ids = ?json_schema_integers
            );

            base_query = base_query.filter(hubuum_class_id.eq_any(json_schema_integers));
        }

        for param in query_params {
            use crate::models::search::{DataType, SearchOperator};
            use crate::{boolean_search, date_search, numeric_search, string_search};
            let field = param.field.as_str();
            let operator = param.operator.clone();
            match field {
                "id" => numeric_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumclass::dsl::id
                ),
                "namespaces" => numeric_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumclass::dsl::namespace_id
                ),
                "created_at" => date_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumclass::dsl::created_at
                ),
                "updated_at" => date_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumclass::dsl::updated_at
                ),
                "name" => string_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumclass::dsl::name
                ),
                "description" => string_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumclass::dsl::description
                ),
                "validate_schema" => boolean_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumclass::dsl::validate_schema
                ),
                "json_schema" => {} // Handled above
                "permission" => {}  // Handled above
                _ => {
                    return Err(ApiError::BadRequest(format!(
                        "Field '{}' isn't searchable (or does not exist) for classes",
                        field
                    )))
                }
            }
        }

        trace_query!(base_query, "Searching classes");

        let result = base_query
            .select(hubuumclass::all_columns())
            .distinct() // TODO: Is it the joins that makes this required?
            .load::<HubuumClass>(&mut conn)?;

        Ok(result)
    }

    async fn search_objects(
        &self,
        pool: &DbPool,
        query_params: Vec<ParsedQueryParam>,
    ) -> Result<Vec<HubuumObject>, ApiError> {
        use crate::models::PermissionFilter;
        use crate::schema::hubuumobject::dsl::{
            hubuum_class_id, hubuumobject, id as hubuum_object_id,
            namespace_id as hubuum_object_nid,
        };
        use crate::schema::permissions::dsl::*;

        debug!(
            message = "Searching objects",
            stage = "Starting",
            user_id = self.id(),
            query_params = ?query_params
        );

        let mut conn = pool.get()?;
        let group_id_subquery = self.group_ids_subquery();

        // Get all namespace IDs that the user has read permissions on, and if we have a list of selected namespaces, filter on those.
        let namespace_ids: Vec<i32> = self
            .namespaces_read(pool)
            .await?
            .into_iter()
            .map(|n| n.id)
            .collect();

        debug!(
            message = "Searching objects",
            stage = "Namespace IDs",
            user_id = self.id(),
            namespace_ids = ?namespace_ids
        );

        let mut base_query = permissions
            .into_boxed()
            .filter(group_id.eq_any(group_id_subquery));

        // Handle permissions
        for perm in query_params.permissions()? {
            base_query = perm.create_boxed_filter(base_query, true);
        }

        let mut base_query =
            base_query.inner_join(hubuumobject.on(hubuum_object_nid.eq_any(namespace_ids)));

        let json_data_queries = query_params.json_datas()?;
        if !json_data_queries.is_empty() {
            debug!(
                message = "Searching classes",
                stage = "JSON Data",
                user_id = self.id(),
                query_params = ?json_data_queries
            );

            let json_data_integers = self.json_data_subquery(pool, json_data_queries)?;

            if json_data_integers.is_empty() {
                debug!(
                    message = "Searching objects",
                    stage = "JSON Data",
                    user_id = self.id(),
                    result = "No object IDs found, returning empty result"
                );
                return Ok(vec![]);
            }

            debug!(
                message = "Searching objects",
                stage = "JSON Data",
                user_id = self.id(),
                result = "Found object IDs",
                class_ids = ?json_data_integers
            );

            base_query = base_query.filter(hubuum_class_id.eq_any(json_data_integers));
        }

        for param in query_params {
            use crate::models::search::{DataType, SearchOperator};
            use crate::{boolean_search, date_search, numeric_search, string_search};
            let field = param.field.as_str();
            let operator = param.operator.clone();
            match field {
                "id" => numeric_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumobject::dsl::id
                ),
                "namespaces" => numeric_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumobject::dsl::namespace_id
                ),
                "created_at" => date_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumobject::dsl::created_at
                ),
                "updated_at" => date_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumobject::dsl::updated_at
                ),
                "name" => string_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumobject::dsl::name
                ),
                "description" => string_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumobject::dsl::description
                ),
                "classes" => numeric_search!(
                    base_query,
                    param,
                    operator,
                    crate::schema::hubuumobject::dsl::hubuum_class_id
                ),
                "json_data" => {}  // Handled above
                "permission" => {} // Handled above
                _ => {
                    return Err(ApiError::BadRequest(format!(
                        "Field '{}' isn't searchable (or does not exist) for classes",
                        field
                    )))
                }
            }
        }

        trace_query!(base_query, "Searching objects");

        let result = base_query
            .select(hubuumobject::all_columns())
            .distinct() // TODO: Is it the joins that makes this required?
            .load::<HubuumObject>(&mut conn)?;

        Ok(result)
    }
}

pub trait GroupAccessors: SelfAccessors<User> {
    /// Return all groups that the user is a member of.
    async fn groups(&self, pool: &DbPool) -> Result<Vec<Group>, ApiError> {
        use crate::schema::groups::dsl::*;
        use crate::schema::user_groups::dsl::{group_id, user_groups, user_id};

        let mut conn = pool.get()?;
        let group_list = user_groups
            .inner_join(groups.on(id.eq(group_id)))
            .filter(user_id.eq(self.id()))
            .select(groups::all_columns())
            .load::<Group>(&mut conn)?;

        Ok(group_list)
    }

    /*
      async fn group_ids(&self, pool: &DbPool) -> Result<Vec<i32>, ApiError> {
          use crate::schema::user_groups::dsl::{group_id, user_groups, user_id};

          let mut conn = pool.get()?;
          let group_list = user_groups
              .filter(user_id.eq(self.id()))
              .select(group_id)
              .load::<i32>(&mut conn)?;

          Ok(group_list)
      }
    */

    /// Generate a subquery to get all group IDs for a user.
    ///
    /// Note that this does not execute the query, it only creates it.
    ///
    /// ## Example
    ///
    /// Check if a user has a specific class permission to a given namespace ID
    ///
    /// ```
    /// let group_id_subquery = user_id.group_ids_subquery();
    ///
    /// let base_query = classpermissions
    /// .into_boxed()
    /// .filter(namespace_id.eq(self.namespace_id))
    /// .filter(group_id.eq_any(group_id_subquery));
    ///
    /// let result = PermissionFilter::filter(permission, base_query)
    /// .first::<ClassPermission>(&mut conn)
    /// .optional()?;
    /// ```
    ///
    fn group_ids_subquery<'a>(
        &self,
    ) -> crate::schema::user_groups::BoxedQuery<'a, diesel::pg::Pg, diesel::sql_types::Integer>
    {
        use crate::schema::user_groups::dsl::*;
        user_groups
            .filter(user_id.eq(self.id()))
            .select(group_id)
            .into_boxed()
    }

    fn json_schema_subquery(
        &self,
        pool: &DbPool,
        json_schema_query_params: Vec<&ParsedQueryParam>,
    ) -> Result<Vec<i32>, ApiError> {
        use crate::models::class::ClassIdResult;
        use crate::models::search::{Operator, SQLValue};

        if json_schema_query_params.is_empty() {
            return Err(ApiError::BadRequest(
                "No json_schema query parameters provided".to_string(),
            ));
        }

        let raw_sql_prefix = "select id from hubuumclass where";
        let mut raw_sql_clauses: Vec<String> = vec![];
        let mut bind_varaibles: Vec<SQLValue> = vec![];

        for param in json_schema_query_params {
            let clause = param.as_json_sql()?;
            debug!(message = "JSON Schema subquery", stage = "Clause", clause = ?clause);
            raw_sql_clauses.push(clause.sql);
            bind_varaibles.extend(clause.bind_variables);
        }

        let raw_sql = format!("{} {}", raw_sql_prefix, raw_sql_clauses.join(" and "))
            .replace_question_mark_with_indexed_n();

        debug!(message = "JSON Schema subquery", stage = "Complete", raw_sql = ?raw_sql, bind_variables = ?bind_varaibles);

        let mut connection = pool.get()?;

        let mut query = diesel::sql_query(raw_sql).into_boxed();

        for bind_var in bind_varaibles {
            match bind_var {
                SQLValue::Integer(i) => query = query.bind::<diesel::sql_types::Integer, _>(i),
                SQLValue::String(s) => query = query.bind::<diesel::sql_types::Text, _>(s),
                SQLValue::Boolean(b) => query = query.bind::<diesel::sql_types::Bool, _>(b),
                SQLValue::Float(f) => query = query.bind::<diesel::sql_types::Float8, _>(f),
                SQLValue::Date(d) => query = query.bind::<diesel::sql_types::Timestamp, _>(d),
            }
        }

        trace_query!(query, "JSONB Schema subquery");

        let result_ids = query.get_results::<ClassIdResult>(&mut connection)?;
        let ids: Vec<i32> = result_ids
            .into_iter()
            .map(|r: ClassIdResult| r.id)
            .collect();

        Ok(ids)
    }

    fn json_data_subquery(
        &self,
        pool: &DbPool,
        json_schema_query_params: Vec<&ParsedQueryParam>,
    ) -> Result<Vec<i32>, ApiError> {
        use crate::models::object::ObjectIDResult;
        use crate::models::search::{Operator, SQLValue};

        if json_schema_query_params.is_empty() {
            return Err(ApiError::BadRequest(
                "No json_data query parameters provided".to_string(),
            ));
        }

        let raw_sql_prefix = "select id from hubuumobject where";
        let mut raw_sql_clauses: Vec<String> = vec![];
        let mut bind_varaibles: Vec<SQLValue> = vec![];

        for param in json_schema_query_params {
            let clause = param.as_json_sql()?;
            debug!(message = "JSON Data subquery", stage = "Clause", clause = ?clause);
            raw_sql_clauses.push(clause.sql);
            bind_varaibles.extend(clause.bind_variables);
        }

        let raw_sql = format!("{} {}", raw_sql_prefix, raw_sql_clauses.join(" and "))
            .replace_question_mark_with_indexed_n();

        debug!(message = "JSON Data subquery", stage = "Complete", raw_sql = ?raw_sql, bind_variables = ?bind_varaibles);

        let mut connection = pool.get()?;

        let mut query = diesel::sql_query(raw_sql).into_boxed();

        for bind_var in bind_varaibles {
            match bind_var {
                SQLValue::Integer(i) => query = query.bind::<diesel::sql_types::Integer, _>(i),
                SQLValue::String(s) => query = query.bind::<diesel::sql_types::Text, _>(s),
                SQLValue::Boolean(b) => query = query.bind::<diesel::sql_types::Bool, _>(b),
                SQLValue::Float(f) => query = query.bind::<diesel::sql_types::Float8, _>(f),
                SQLValue::Date(d) => query = query.bind::<diesel::sql_types::Timestamp, _>(d),
            }
        }

        trace_query!(query, "JSONB Data subquery");

        let result_ids = query.get_results::<ObjectIDResult>(&mut connection)?;
        let ids: Vec<i32> = result_ids
            .into_iter()
            .map(|r: ObjectIDResult| r.id)
            .collect();

        Ok(ids)
    }
}

pub trait UserNamespaceAccessors: SelfAccessors<User> + GroupAccessors {
    /// Return all namespaces that the user has NamespacePermissions::ReadCollection on.
    async fn namespaces_read(&self, pool: &DbPool) -> Result<Vec<Namespace>, ApiError> {
        self.namespaces(pool, vec![Permissions::ReadCollection])
            .await
    }

    async fn namespaces(
        &self,
        pool: &DbPool,
        permissions_list: Vec<Permissions>,
    ) -> Result<Vec<Namespace>, ApiError> {
        use crate::models::PermissionFilter;
        use crate::schema::namespaces::dsl::{id as namespaces_table_id, namespaces};
        use crate::schema::permissions::dsl::{group_id, namespace_id, permissions};

        let mut conn = pool.get()?;

        let groups_id_subquery = self.group_ids_subquery();

        let mut base_query = permissions
            .into_boxed()
            .filter(group_id.eq_any(groups_id_subquery));

        for perm in permissions_list {
            base_query = perm.create_boxed_filter(base_query, true);
        }

        let result = base_query
            .inner_join(namespaces.on(namespace_id.eq(namespaces_table_id)))
            .select(namespaces::all_columns())
            .load::<Namespace>(&mut conn)?;

        Ok(result)
    }
}

pub trait UserClassAccessors: Search {
    async fn classes_read(&self, pool: &DbPool) -> Result<Vec<HubuumClass>, ApiError> {
        self.search_classes(
            pool,
            vec![ParsedQueryParam::new("permission", None, "ReadClass")],
        )
        .await
    }

    async fn classes_read_within_namespaces<N: NamespaceAccessors>(
        &self,
        pool: &DbPool,
        namespaces: Vec<N>,
    ) -> Result<Vec<HubuumClass>, ApiError> {
        let futures: Vec<_> = namespaces
            .into_iter()
            .map(|n| {
                let pool_ref = &pool;
                async move { n.namespace_id(pool_ref).await }
            })
            .collect();
        let namespace_ids: Vec<i32> = try_join_all(futures).await?;

        let mut queries = vec![ParsedQueryParam::new("permission", None, "ReadClass")];
        for nid in namespace_ids {
            queries.push(ParsedQueryParam::new("namespace", None, &nid.to_string()));
        }

        self.search_classes(pool, queries).await
    }

    async fn classes_within_namespaces_with_permissions<N: NamespaceAccessors>(
        &self,
        pool: &DbPool,
        namespaces: Vec<N>,
        permissions_list: Vec<Permissions>,
    ) -> Result<Vec<HubuumClass>, ApiError> {
        let futures: Vec<_> = namespaces
            .into_iter()
            .map(|n| {
                let pool_ref = &pool;
                async move { n.namespace_id(pool_ref).await }
            })
            .collect();
        let namespace_ids: Vec<i32> = try_join_all(futures).await?;

        let mut queries = vec![];
        for nid in namespace_ids {
            queries.push(ParsedQueryParam::new("namespace", None, &nid.to_string()));
        }

        for perm in permissions_list {
            queries.push(ParsedQueryParam::new("permission", None, &perm.to_string()));
        }

        self.search_classes(pool, queries).await
    }

    async fn classes_with_permissions(
        &self,
        pool: &DbPool,
        permissions_list: Vec<Permissions>,
    ) -> Result<Vec<HubuumClass>, ApiError> {
        let mut queries = vec![];

        for perm in permissions_list {
            queries.push(ParsedQueryParam::new("permission", None, &perm.to_string()));
        }

        self.search_classes(pool, queries).await
    }

    async fn classes(&self, pool: &DbPool) -> Result<Vec<HubuumClass>, ApiError> {
        self.search_classes(pool, vec![]).await
    }
}

pub trait ObjectAccessors: UserClassAccessors + UserNamespaceAccessors {
    async fn objects_in_class_read<C: UserClassAccessors>(
        &self,
        pool: &DbPool,
        class_id: C,
    ) -> Result<Vec<HubuumObject>, ApiError> {
        self.objects_in_classes_read(pool, vec![class_id]).await
    }

    async fn objects_in_classes_read<C: UserClassAccessors>(
        &self,
        pool: &DbPool,
        class_ids: Vec<C>,
    ) -> Result<Vec<HubuumObject>, ApiError> {
        self.objects(pool, class_ids, vec![Permissions::ReadClass])
            .await
    }

    async fn objects<C: UserClassAccessors>(
        &self,
        pool: &DbPool,
        class_ids: Vec<C>,
        permissions_list: Vec<Permissions>,
    ) -> Result<Vec<HubuumObject>, ApiError> {
        use crate::models::PermissionFilter;
        use crate::schema::hubuumobject::dsl::{
            hubuum_class_id, hubuumobject, namespace_id as hubuumobject_nid,
        };
        use crate::schema::permissions::dsl::*;

        let mut conn = pool.get()?;
        let group_id_subquery = self.group_ids_subquery();

        let namespace_ids: Vec<i32> = self
            .namespaces_read(pool)
            .await?
            .iter()
            .map(|n| n.id)
            .collect();

        let mut base_query = permissions
            .into_boxed()
            .filter(namespace_id.eq_any(namespace_ids.clone()))
            .filter(group_id.eq_any(group_id_subquery));

        for perm in permissions_list {
            base_query = perm.create_boxed_filter(base_query, true);
        }

        let mut joined_query =
            base_query.inner_join(hubuumobject.on(hubuumobject_nid.eq_any(namespace_ids)));

        if !class_ids.is_empty() {
            let valid_class_ids = class_ids.iter().map(|c| c.id()).collect::<Vec<i32>>();
            joined_query = joined_query.filter(hubuum_class_id.eq_any(valid_class_ids));
        }

        let result = joined_query
            .select(hubuumobject::all_columns())
            .load::<HubuumObject>(&mut conn)?;

        Ok(result)
    }
}

impl UserNamespaceAccessors for User {}
impl UserNamespaceAccessors for UserID {}

impl UserClassAccessors for User {}
impl UserClassAccessors for UserID {}

impl GroupAccessors for User {}
impl GroupAccessors for UserID {}

impl Search for User {}
impl Search for UserID {}

impl SelfAccessors<User> for User {
    fn id(&self) -> i32 {
        self.id
    }

    async fn instance(&self, _pool: &DbPool) -> Result<User, ApiError> {
        Ok(self.clone())
    }
}

impl SelfAccessors<User> for UserID {
    fn id(&self) -> i32 {
        self.0
    }

    async fn instance(&self, pool: &DbPool) -> Result<User, ApiError> {
        use crate::schema::users::dsl::*;
        Ok(users
            .filter(id.eq(self.0))
            .first::<User>(&mut pool.get()?)?)
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::models::{GroupID, NewHubuumClass, Permissions, PermissionsList};
    use crate::tests::{
        create_test_group, create_test_user, ensure_admin_group, ensure_admin_user,
        setup_pool_and_tokens,
    };
    use crate::traits::PermissionController;
    use crate::traits::{CanDelete, CanSave};
    use crate::{assert_contains, assert_not_contains};

    #[actix_rt::test]
    async fn test_user_permissions_namespace_and_class_listing() {
        use crate::models::namespace::NewNamespace;

        let (pool, _, _) = setup_pool_and_tokens().await;
        let test_user_1 = create_test_user(&pool).await;
        let test_group_1 = create_test_group(&pool).await;
        let test_user_2 = create_test_user(&pool).await;
        let test_group_2 = create_test_group(&pool).await;

        test_group_1.add_member(&pool, &test_user_1).await.unwrap();
        test_group_2.add_member(&pool, &test_user_2).await.unwrap();

        let ns = NewNamespace {
            name: "test_user_namespace_listing".to_string(),
            description: "Test namespace".to_string(),
        }
        .save_and_grant_all_to(&pool, GroupID(test_group_1.id))
        .await
        .unwrap();

        let class = NewHubuumClass {
            name: "test_user_namespace_listing".to_string(),
            description: "Test class".to_string(),
            json_schema: serde_json::json!({}),
            validate_schema: false,
            namespace_id: ns.id,
        }
        .save(&pool)
        .await
        .unwrap();

        class
            .grant(
                &pool,
                test_group_1.id,
                PermissionsList::new([
                    Permissions::ReadClass,
                    Permissions::UpdateClass,
                    Permissions::DeleteClass,
                    Permissions::CreateObject,
                ]),
            )
            .await
            .unwrap();

        let nslist = test_user_1.namespaces_read(&pool).await.unwrap();
        assert_contains!(&nslist, &ns);

        let nslist = test_user_2.namespaces_read(&pool).await.unwrap();
        assert_not_contains!(&nslist, &ns);

        let classlist = test_user_1.classes_read(&pool).await.unwrap();
        assert_contains!(&classlist, &class);

        let classlist = test_user_2.classes_read(&pool).await.unwrap();
        assert_not_contains!(&classlist, &class);

        ns.grant_one(&pool, test_group_2.id, Permissions::ReadCollection)
            .await
            .unwrap();

        let nslist = test_user_2.namespaces_read(&pool).await.unwrap();
        assert_contains!(&nslist, &ns);

        let classlist = test_user_1.classes_read(&pool).await.unwrap();
        assert_contains!(&classlist, &class);

        class
            .grant_one(&pool, test_group_2.id, Permissions::ReadClass)
            .await
            .unwrap();

        let classlist = test_user_2.classes_read(&pool).await.unwrap();
        assert_contains!(&classlist, &class);

        class
            .revoke_one(&pool, test_group_2.id, Permissions::ReadClass)
            .await
            .unwrap();

        let classlist = test_user_2.classes_read(&pool).await.unwrap();
        assert_not_contains!(&classlist, &class);

        let nslist = test_user_2.namespaces_read(&pool).await.unwrap();
        assert_contains!(&nslist, &ns);

        ns.revoke_all(&pool, test_group_2.id).await.unwrap();

        let nslist = test_user_2.namespaces_read(&pool).await.unwrap();
        assert_not_contains!(&nslist, &ns);

        test_user_1.delete(&pool).await.unwrap();
        test_user_2.delete(&pool).await.unwrap();
        test_group_1.delete(&pool).await.unwrap();
        test_group_2.delete(&pool).await.unwrap();
        ns.delete(&pool).await.unwrap();
    }
}
