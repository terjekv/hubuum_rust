#[cfg(test)]

mod test {
    use crate::models::class::NewHubuumClass;
    use crate::models::group::GroupID;
    use crate::models::search::{DataType, ParsedQueryParam, SearchOperator};
    use crate::models::{HubuumClass, Namespace, NewNamespace};
    use crate::tests::{ensure_admin_group, ensure_admin_user, setup_pool_and_tokens};
    use crate::traits::{CanDelete, CanSave, SearchClasses};

    struct TestCase {
        query: Vec<ParsedQueryParam>,
        expected: usize,
    }

    async fn setup_test_structure(prefix: &str) -> (Vec<Namespace>, Vec<HubuumClass>) {
        let pretty_prefix = prefix.replace("_", " ");

        let (pool, _, _) = setup_pool_and_tokens().await;
        let admin_group = ensure_admin_group(&pool).await;

        let mut namespaces = vec![];
        let mut classes = vec![];

        for i in 0..3 {
            let padded_i = format!("{:02}", i);
            let namespace_name = format!("{}_namespace_{}", prefix, padded_i);
            let namespace_description = format!("{} namespace {}", pretty_prefix, padded_i);

            namespaces.push(
                NewNamespace {
                    name: namespace_name,
                    description: namespace_description,
                }
                .save_and_grant_all_to(&pool, GroupID(admin_group.id))
                .await
                .unwrap(),
            );
        }

        for i in 0..10 {
            let padded_i = format!("{:02}", i);
            let mut nid = namespaces[0].id;
            if i > 8 {
                nid = namespaces[2].id; // We'll get one class in this namespace (9)
            } else if i > 5 {
                nid = namespaces[1].id; // We'll get three classes in this namespace (6,7,8)
            }

            classes.push(
                NewHubuumClass {
                    name: format!("{}_class_{}", prefix, padded_i),
                    description: format!("{} class {}", pretty_prefix, padded_i),
                    json_schema: serde_json::json!({}),
                    validate_schema: false,
                    namespace_id: nid,
                }
                .save(&pool)
                .await
                .unwrap(),
            );
        }

        (namespaces, classes)
    }

    async fn check_test_cases(testcases: Vec<TestCase>) {
        let (pool, _, _) = setup_pool_and_tokens().await;
        let admin_user = ensure_admin_user(&pool).await;

        for tc in testcases {
            let hits = admin_user
                .search_classes(&pool, tc.query.clone())
                .await
                .unwrap();
            assert_eq!(
                hits.len(),
                tc.expected,
                "Query: {:?}, Hits: {:?}",
                tc.query,
                hits
            );
        }
    }

    async fn cleanup(namespaces: Vec<Namespace>) {
        let (pool, _, _) = setup_pool_and_tokens().await;
        for ns in namespaces {
            ns.delete(&pool).await.unwrap();
        }
    }

    #[actix_rt::test]
    async fn test_equals() {
        let (namespaces, classes) = setup_test_structure("test_user_class_equals").await;

        let testcases = vec![
            TestCase {
                query: vec![ParsedQueryParam::new(
                    "id",
                    Some(SearchOperator::Equals { is_negated: false }),
                    &classes[0].id.to_string(),
                )],
                expected: 1,
            },
            TestCase {
                query: vec![ParsedQueryParam::new(
                    "name",
                    Some(SearchOperator::Equals { is_negated: false }),
                    &classes[0].name,
                )],
                expected: 1,
            },
            TestCase {
                query: vec![ParsedQueryParam::new(
                    "namespaces",
                    Some(SearchOperator::Equals { is_negated: false }),
                    &namespaces[2].id.to_string(),
                )],
                expected: 1,
            },
            TestCase {
                query: vec![ParsedQueryParam::new(
                    "validate_schema",
                    Some(SearchOperator::Equals { is_negated: false }),
                    "true",
                )],
                expected: 0,
            },
            TestCase {
                query: vec![
                    ParsedQueryParam::new(
                        "validate_schema",
                        Some(SearchOperator::Equals { is_negated: true }), // so true becomes false
                        "true",
                    ),
                    ParsedQueryParam::new(
                        "namespaces",
                        Some(SearchOperator::Equals { is_negated: false }),
                        &namespaces[2].id.to_string(),
                    ),
                ],
                expected: 1,
            },
        ];

        check_test_cases(testcases).await;
        cleanup(namespaces).await;
    }

    #[actix_rt::test]
    async fn test_class_search() {
        let (namespaces, classes) = setup_test_structure("test_user_class_search").await;

        let testcases = vec![
            TestCase {
                query: vec![ParsedQueryParam::new(
                    "id",
                    Some(SearchOperator::Equals { is_negated: false }),
                    &classes[0].id.to_string(),
                )],
                expected: 1,
            },
            TestCase {
                query: vec![ParsedQueryParam::new(
                    "namespaces",
                    Some(SearchOperator::Equals { is_negated: false }),
                    &namespaces[2].id.to_string(),
                )],
                expected: 1,
            },
            TestCase {
                query: vec![
                    ParsedQueryParam::new(
                        "id",
                        Some(SearchOperator::Gt {
                            data_type: DataType::NumericOrDate,
                            is_negated: false,
                        }),
                        &classes[1].id.to_string(),
                    ),
                    ParsedQueryParam::new(
                        "id",
                        Some(SearchOperator::Lt {
                            data_type: DataType::NumericOrDate,
                            is_negated: false,
                        }),
                        &classes[3].id.to_string(),
                    ),
                ],
                expected: 1,
            },
            TestCase {
                query: vec![ParsedQueryParam::new(
                    "name",
                    Some(SearchOperator::Contains {
                        data_type: DataType::String,
                        is_negated: false,
                    }),
                    "class_search",
                )],
                expected: 10,
            },
            TestCase {
                query: vec![
                    ParsedQueryParam::new(
                        "name",
                        Some(SearchOperator::IContains {
                            data_type: DataType::String,
                            is_negated: false,
                        }),
                        "CLASS_search",
                    ),
                    ParsedQueryParam::new(
                        "namespaces",
                        Some(SearchOperator::Equals { is_negated: false }),
                        &namespaces[1].id.to_string(),
                    ),
                ],
                expected: 3,
            },
            TestCase {
                query: vec![
                    ParsedQueryParam::new(
                        "description",
                        Some(SearchOperator::Contains {
                            data_type: DataType::String,
                            is_negated: false,
                        }),
                        "class search",
                    ),
                    ParsedQueryParam::new(
                        "validate_schema",
                        Some(SearchOperator::Equals { is_negated: false }),
                        "true",
                    ),
                ],
                expected: 0,
            },
        ];

        check_test_cases(testcases).await;
        cleanup(namespaces).await;
    }

    #[actix_rt::test]
    async fn test_search_int_ranges() {
        let (namespaces, _) = setup_test_structure("test_user_class_int_ranges").await;

        let testcases = vec![
            TestCase {
                query: vec![ParsedQueryParam::new(
                    "namespaces",
                    Some(SearchOperator::Equals { is_negated: false }),
                    format!("{}-{}", namespaces[1].id, namespaces[2].id).as_str(),
                )],
                expected: 4,
            },
            TestCase {
                query: vec![ParsedQueryParam::new(
                    "namespaces",
                    Some(SearchOperator::Equals { is_negated: false }),
                    format!("{},{}", namespaces[0].id, namespaces[2].id).as_str(),
                )],
                expected: 7,
            },
            TestCase {
                query: vec![ParsedQueryParam::new(
                    "namespaces",
                    Some(SearchOperator::Equals { is_negated: false }),
                    format!(
                        "{},{},{}",
                        namespaces[0].id, namespaces[1].id, namespaces[2].id
                    )
                    .as_str(),
                )],
                expected: 10,
            },
            TestCase {
                query: vec![ParsedQueryParam::new(
                    "namespaces",
                    Some(SearchOperator::Equals { is_negated: false }),
                    format!("{}-{}", namespaces[0].id, namespaces[2].id).as_str(),
                )],
                expected: 10,
            },
            TestCase {
                query: vec![ParsedQueryParam::new(
                    "namespaces",
                    Some(SearchOperator::Equals { is_negated: false }),
                    format!(
                        "{}-{},{}",
                        namespaces[0].id, namespaces[1].id, namespaces[2].id
                    )
                    .as_str(),
                )],
                expected: 10,
            },
        ];

        check_test_cases(testcases).await;
        cleanup(namespaces).await;
    }
}
