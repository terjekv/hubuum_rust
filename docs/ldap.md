# LDAP

Hubuum supports using LDAP for authentication and authorization. This is done by authenticating as an LDAP user to a specific LDAP domain as per the configuration.

## Configuration

The following configuration options are available for LDAP:

### ldap_urls (HUBUUM_LDAP_URLS)

A comma separated list of LDAP labels and their URLs. The label is used to identify the LDAP domain in the configuration. The URL is the URL to the LDAP server, base64-encoded. Assume we have two LDAP setups we want to use for logging in, `default` with the URL `ldap://localhost:389` and `other` with `ldaps://other:636`. The configuration would then look like this:

Command line: `--ldap_urls=default=bGRhcDovL2xvY2FsaG9zdDozODkK,other=bGRhcHM6Ly9vdGhlcjo2MzYK`
Environment variable: `HUBUUM_LDAP_BIND_DN=default=bGRhcDovL2xvY2FsaG9zdDozODkK,other=bGRhcHM6Ly9vdGhlcjo2MzYK`

#### ldap_bind_dn (HUBUUM_LDAP_BIND_DN)

A comma separated list of LDAP labels and their bind DNs. The label is used to identify the LDAP domain in the configuration. The bind DN is the DN to use when binding to the LDAP server, base64-encoded. Assume we have two LDAP setups we want to use for logging in, `default` with the bind DN `dc=example,dc=com` and `other` with `dc=other,dc=org`. The configuration would then look like this:

Command line: `--ldap_bind_dn=default=ZGM9ZXhhbXBsZSxkYz1jb20K,other=ZGM9b3RoZXIsZGM9b3JnCg==`
Environment variable: `HUBUUM_LDAP_BIND_DN=default=ZGM9ZXhhbXBsZSxkYz1jb20K,other=ZGM9b3RoZXIsZGM9b3JnCg==`

#### ldap_system_users

A comma separated list of LDAP labels and their system users and their password. The username and password are separated with a semicolon, and both have to be base64 encoded. The label is used to identify the LDAP domain in the configuration. The system user is the user that is used to search for users in the LDAP server. If this is not set, the credentials for the user logging in will be used.

Assume we have two LDAP setups we want add system users for, `default` with the system user `service` and the password `secret` and `other` with the system user `svc` and the password `password`. The configuration would then look like this:

Command line: `--ldap_system_users=c2VydmljZQo=;c2VjcmV0,other=c3ZjCg==;cGFzc3dvcmQK`
Environment variable: `HUBUUM_LDAP_SYSTEM_USERS=c2VydmljZQo=;c2VjcmV0,other=c3ZjCg==;cGFzc3dvcmQK`
