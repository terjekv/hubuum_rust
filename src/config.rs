use base64::{engine::general_purpose::STANDARD, Engine as _};
use clap::Parser;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::str;
use tokio::sync::Mutex;
use tracing_subscriber::filter::EnvFilter;

use crate::utilities::is_valid_log_level;

const PORT_RANGE: RangeInclusive<usize> = 1..=65535;

pub static CONFIG: Lazy<Mutex<AppConfig>> = Lazy::new(|| Mutex::new(AppConfig::new()));

pub async fn get_config() -> tokio::sync::MutexGuard<'static, AppConfig> {
    CONFIG.lock().await
}

#[derive(Clone, Debug, Serialize)]
pub struct LDAPConfig {
    pub url: String,
    pub bind_dn: String,
    pub system_account: Option<(String, String)>,
}

#[derive(Parser, Debug, Deserialize, Serialize, Clone)]
pub struct ClapConfig {
    /// IP address to bind to, use '*' for all interfaces.
    #[clap(long, env = "HUBUUM_BIND_IP", default_value = "127.0.0.1")]
    pub bind_ip: String,

    /// Netowrk port to bind to.
    #[clap(long, env = "HUBUUM_BIND_PORT", default_value = "8080", value_parser = port_in_range)]
    pub port: u16,

    /// Logging level, as text.
    /// Valid values are: trace, debug, info, warn, error
    #[clap(
        long,
        env = "HUBUUM_LOG_LEVEL",
        default_value = "info",
        verbatim_doc_comment,
        value_parser = valid_log_level
    )]
    pub log_level: String,

    /// Database URL
    #[clap(
        long,
        env = "HUBUUM_DATABASE_URL",
        default_value = "postgres://localhost"
    )]
    pub database_url: String,

    /// Number of Actix workers
    #[clap(long, env = "HUBUUM_ACTIX_WORKERS", default_value_t = 4)]
    pub actix_workers: usize,

    /// Number of DB connections in the pool
    #[clap(long, env = "HUBUUM_DB_POOL_SIZE", default_value_t = 10)]
    pub db_pool_size: u32,

    /// LDAP URLs
    /// Format is label1:ldap://host:port,label2:ldaps://host:port
    /// NOTE: The URLs have to be BASE64 encoded
    #[clap(long, env = "HUBUUM_LDAP_URLS", verbatim_doc_comment)]
    pub ldap_urls: Option<String>,

    /// LDAP bind DN to to find users. Scope::Subtree is assumed
    /// Format is label1:DN,label2:DN
    /// NOTE: The DN has to be BASE64 encoded
    #[clap(
        long,
        env = "HUBUUM_LDAP_BIND_DN",
        verbatim_doc_comment,
        requires = "ldap_urls"
    )]
    pub ldap_bind_dn: Option<String>,

    /// LDAP system users and passwords, used for user searches with the given DN.
    /// If not set, the username and password for each login will be used.
    /// Format is label1:username1;password1,label2:username2;password2
    /// NOTE: Both the usernames and passwords have to be BASE64 encoded
    #[clap(
        long,
        env = "HUBUUM_LDAP_SYSTEM_USERS",
        requires = "ldap_bind_dn",
        verbatim_doc_comment
    )]
    pub ldap_system_users: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct AppConfig {
    pub bind_ip: String,
    pub port: u16,
    pub log_level: String,
    pub database_url: String,
    pub actix_workers: usize,
    pub db_pool_size: u32,

    pub ldap_configs: HashMap<String, LDAPConfig>,
}

impl AppConfig {
    pub fn new() -> Self {
        let app_config = ClapConfig::parse().try_into();
        match app_config {
            Ok(app_config) => app_config,
            Err(e) => {
                println!("Error parsing config: {}", e);
                std::process::exit(1);
            }
        }
    }

    pub fn get_log_level_as_filter(&self) -> EnvFilter {
        EnvFilter::try_new(&self.log_level).unwrap_or_else(|_e| {
            println!("Error parsing log level: {}", &self.log_level);
            std::process::exit(1);
        })
    }

    pub fn get_ldap_config(&self, label: &str) -> Option<&LDAPConfig> {
        self.ldap_configs.get(label)
    }
}

impl TryFrom<ClapConfig> for AppConfig {
    type Error = ConfigError;

    fn try_from(clap_config: ClapConfig) -> Result<Self, ConfigError> {
        let ldap_configs = clap_config.parse_ldap_configs()?;

        Ok(AppConfig {
            bind_ip: clap_config.bind_ip,
            port: clap_config.port,
            log_level: clap_config.log_level,
            database_url: clap_config.database_url,
            actix_workers: clap_config.actix_workers,
            db_pool_size: clap_config.db_pool_size,

            ldap_configs,
        })
    }
}

impl ClapConfig {
    pub fn parse_ldap_configs(&self) -> Result<HashMap<String, LDAPConfig>, ConfigError> {
        let ldap_urls = match &self.ldap_urls {
            Some(urls) => split_and_decode_ldap_data(urls)?,
            None => return Ok(HashMap::new()),
        };

        let ldap_bind_dn = match &self.ldap_bind_dn {
            Some(bind_dn) => split_and_decode_ldap_data(bind_dn)?,
            None => HashMap::new(),
        };

        for label in ldap_urls.keys() {
            if !ldap_bind_dn.contains_key(label) {
                return Err(ConfigError::MissingLDAPData(format!(
                    "LDAP bind DN is missing for label {}",
                    label
                )));
            }
        }

        let ldap_system_users = match &self.ldap_system_users {
            Some(system_users) => split_and_decode_ldap_data(system_users)?,
            None => HashMap::new(),
        };

        let mut ldap_configs = HashMap::new();
        for (label, url) in ldap_urls {
            let bind_dn = ldap_bind_dn
                .get(&label)
                .ok_or_else(|| {
                    ConfigError::MissingLDAPData(format!(
                        "LDAP bind DN is missing for label {}",
                        label
                    ))
                })?
                .to_string();

            let system_account = ldap_system_users
                .get(&label)
                .map(|sys_user| {
                    let mut parts = sys_user.split(';');
                    let username = parts.next().ok_or_else(|| {
                        ConfigError::MissingLDAPData(format!(
                            "LDAP system user is missing username for label {}",
                            label
                        ))
                    })?;
                    let password = parts.next().ok_or_else(|| {
                        ConfigError::MissingLDAPData(format!(
                            "LDAP system user is missing password for label {}",
                            label
                        ))
                    })?;

                    Ok((username.to_string(), password.to_string()))
                })
                .transpose()?;

            ldap_configs.insert(
                label,
                LDAPConfig {
                    url,
                    bind_dn,
                    system_account,
                },
            );
        }

        Ok(ldap_configs)
    }
}

// Validators

fn port_in_range(s: &str) -> Result<u16, String> {
    let port: usize = s
        .parse()
        .map_err(|_| format!("`{s}` isn't a port number"))?;
    if PORT_RANGE.contains(&port) {
        Ok(port as u16)
    } else {
        Err(format!(
            "port not in range {}-{}",
            PORT_RANGE.start(),
            PORT_RANGE.end()
        ))
    }
}

fn valid_log_level(log_level: &str) -> Result<String, String> {
    if is_valid_log_level(log_level) {
        Ok(log_level.to_string())
    } else {
        Err(format!("Invalid log level: {}", log_level))
    }
}

// Utility functions

fn split_and_decode_ldap_data(data: &str) -> Result<HashMap<String, String>, ConfigError> {
    let mut ldap_data: HashMap<String, String> = HashMap::new();

    for entry in data.split(',') {
        let parts: Vec<&str> = entry.split(':').collect();
        let num_parts = parts.len();

        if num_parts != 2 {
            return Err(ConfigError::MissingLDAPData(
                format!(
                    "Invalid label/value pair to decode, found {} part(s), expected 2 parts",
                    num_parts
                )
                .to_string(),
            ));
        }

        let label = parts[0];

        let decoded_value = match STANDARD.decode(parts[1].as_bytes()) {
            Ok(decoded_value) => decoded_value,
            Err(e) => {
                return Err(ConfigError::DecodeError(format!(
                    "Error decoding LDAP data for label {} ({})",
                    label, e
                )))
            }
        };

        let decoded_value = match str::from_utf8(&decoded_value) {
            Ok(decoded_value) => decoded_value,
            Err(e) => {
                return Err(ConfigError::Utf8Error(format!(
                    "Error decoding LDAP data for label {} ({})",
                    label, e
                )))
            }
        };

        ldap_data.insert(label.to_string(), decoded_value.to_string());
    }

    Ok(ldap_data)
}

// Errors

#[derive(Debug)]
pub enum ConfigError {
    MissingLDAPData(String),
    DecodeError(String),
    Utf8Error(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingLDAPData(msg) => write!(f, "Missing LDAP data: {}", msg),
            ConfigError::DecodeError(msg) => write!(f, "Decode error: {}", msg),
            ConfigError::Utf8Error(msg) => write!(f, "UTF-8 error: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_n_field_failure(s: &str, n: usize) {
        let result = split_and_decode_ldap_data(s);
        match result {
            Ok(_) => panic!("Expected error"),
            Err(e) => match e {
                ConfigError::MissingLDAPData(msg) => {
                    assert_eq!(
                        msg,
                        format!(
                            "Invalid label/value pair to decode, found {} part(s), expected 2 parts",
                            n
                        )
                        .to_string()
                    )
                }
                _ => panic!("Unexpected error: {}", e),
            },
        }
    }

    fn compare_input_with_expectations(input: &str, expected_pairs: &[(&str, &str)]) {
        let mut expected_output = HashMap::new();
        for &(key, value) in expected_pairs {
            expected_output.insert(key.to_string(), value.to_string());
        }

        let ldap_data_decoded = split_and_decode_ldap_data(input).unwrap();
        assert_eq!(expected_output, ldap_data_decoded);
    }

    #[test]
    fn test_splitting_correctly() {
        expect_n_field_failure("no_colon_here", 1);
        expect_n_field_failure("this:has:two", 3);
    }

    #[test]
    fn test_decoding_correctly() {
        compare_input_with_expectations(
            "label1:dmFsdWUx,label2:dmFsdWUy",
            &[("label1", "value1"), ("label2", "value2")],
        );
        compare_input_with_expectations(
            "username1:bm90IGEgc2VjcmV0IHBhc3N3b3JkIQ==",
            &[("username1", "not a secret password!")],
        );
    }

    #[test]
    fn test_invalid_value_for_decode() {
        let result = split_and_decode_ldap_data("label1:invalid_base64");
        match result {
            Ok(_) => panic!("Expected error"),
            Err(e) => match e {
                ConfigError::DecodeError(msg) => {
                    assert_eq!(
                        msg,
                        "Error decoding LDAP data for label label1 (Invalid byte 95, offset 7.)"
                            .to_string()
                    )
                }
                _ => panic!("Unexpected error: {}", e),
            },
        }
    }
}
