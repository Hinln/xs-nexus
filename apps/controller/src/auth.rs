use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{config::ControllerConfig, error::ApiError, state::AppState};

const SESSION_COOKIE: &str = "xs_nexus_session";
const SESSION_TOKEN_DOMAIN: &[u8] = b"XS Nexus console session token v1";
const CSRF_TOKEN_DOMAIN: &[u8] = b"XS Nexus console csrf token v1";
const USERNAME_DOMAIN: &[u8] = b"XS Nexus console username v1";
const LOGIN_WINDOW_MINUTES: i64 = 15;
const MAX_LOGIN_FAILURES: i64 = 5;
const MAX_ACTIVE_SESSIONS: i64 = 10;

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("database operation failed")]
    Database(#[source] sqlx::Error),
    #[error("password hashing failed")]
    PasswordHash,
    #[error("secure random generation failed")]
    Random,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConsoleRole {
    Administrator,
    Operator,
    Auditor,
}

impl ConsoleRole {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "administrator" => Some(Self::Administrator),
            "operator" => Some(Self::Operator),
            "auditor" => Some(Self::Auditor),
            _ => None,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Administrator => "administrator",
            Self::Operator => "operator",
            Self::Auditor => "auditor",
        }
    }

    const fn permits(self, permission: Permission) -> bool {
        match permission {
            Permission::Read => true,
            Permission::Manage => matches!(self, Self::Administrator | Self::Operator),
            Permission::ManageUsers => matches!(self, Self::Administrator),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Permission {
    Read,
    Manage,
    ManageUsers,
}

#[derive(Clone)]
pub(crate) struct ManagementActor {
    actor_type: &'static str,
    actor_id: String,
}

impl ManagementActor {
    pub(crate) const fn actor_type(&self) -> &'static str {
        self.actor_type
    }

    pub(crate) fn actor_id(&self) -> &str {
        &self.actor_id
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateConsoleUserRequest {
    username: String,
    display_name: String,
    password: String,
    role: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct ConsoleUserResponse {
    id: Uuid,
    username: String,
    display_name: String,
    role: &'static str,
    enabled: bool,
    created_at: DateTime<Utc>,
    last_login_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub(crate) struct LoginResponse {
    user: ConsoleUserResponse,
    csrf_token: String,
    expires_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub(crate) struct SessionResponse {
    user: ConsoleUserResponse,
    csrf_token: String,
    expires_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct LoginUserRow {
    id: Uuid,
    username: String,
    display_name: String,
    password_hash: String,
    role: String,
    enabled: bool,
    created_at: DateTime<Utc>,
    last_login_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct SessionRow {
    session_id: Uuid,
    csrf_hash: Vec<u8>,
    expires_at: DateTime<Utc>,
    user_id: Uuid,
    username: String,
    display_name: String,
    role: String,
    enabled: bool,
    created_at: DateTime<Utc>,
    last_login_at: Option<DateTime<Utc>>,
}

struct AuthenticatedSession {
    session_id: Uuid,
    csrf_hash: [u8; 32],
    expires_at: DateTime<Utc>,
    user: ConsoleUserResponse,
    role: ConsoleRole,
}

pub(crate) async fn ensure_bootstrap_administrator(
    pool: &PgPool,
    config: &ControllerConfig,
) -> Result<(), BootstrapError> {
    let (Some(username), Some(password)) = (
        config.console_bootstrap_username.as_deref(),
        config.console_bootstrap_password.as_deref(),
    ) else {
        return Ok(());
    };

    let mut transaction = pool.begin().await.map_err(BootstrapError::Database)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('xs-nexus-console-bootstrap'))")
        .execute(&mut *transaction)
        .await
        .map_err(BootstrapError::Database)?;
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM console_users")
        .fetch_one(&mut *transaction)
        .await
        .map_err(BootstrapError::Database)?;
    if users == 0 {
        let password_hash =
            hash_password_for_bootstrap(Zeroizing::new(password.to_owned())).await?;
        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO console_users
             (id, username, display_name, password_hash, role)
             VALUES ($1, $2, '系统管理员', $3, 'administrator')",
        )
        .bind(user_id)
        .bind(username)
        .bind(password_hash)
        .execute(&mut *transaction)
        .await
        .map_err(BootstrapError::Database)?;
        sqlx::query(
            "INSERT INTO audit_events
             (actor_type, actor_id, action, target_type, target_id, outcome, metadata)
             VALUES ('system', 'controller-bootstrap', 'console_user.bootstrap',
                     'console_user', $1, 'success', $2)",
        )
        .bind(user_id.to_string())
        .bind(serde_json::json!({"username": username, "role": "administrator"}))
        .execute(&mut *transaction)
        .await
        .map_err(BootstrapError::Database)?;
    }
    transaction.commit().await.map_err(BootstrapError::Database)
}

pub(crate) async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<LoginResponse>), ApiError> {
    let (user, role) = authenticate_login(&state, request).await?;
    let (session_token, response) = create_login_session(&state, user, role).await?;
    let mut headers = no_store_headers();
    headers.insert(
        header::SET_COOKIE,
        session_cookie(
            &session_token,
            state.console_session_ttl_seconds,
            state.console_cookie_secure,
        )?,
    );
    Ok((headers, Json(response)))
}

async fn authenticate_login(
    state: &AppState,
    request: LoginRequest,
) -> Result<(LoginUserRow, ConsoleRole), ApiError> {
    let username = normalize_username(&request.username).ok_or_else(ApiError::unauthorized)?;
    let username_hash = domain_hash(USERNAME_DOMAIN, username.as_bytes());
    if login_failure_count(&state.pool, &username_hash).await? >= MAX_LOGIN_FAILURES {
        record_login_failure(&state.pool, &username_hash, "rate_limited").await?;
        record_login_audit(&state.pool, &username_hash, "rate_limited").await?;
        return Err(ApiError::rate_limited());
    }

    let user = sqlx::query_as::<_, LoginUserRow>(
        "SELECT id, username, display_name, password_hash, role, enabled,
                created_at, last_login_at
         FROM console_users
         WHERE username = $1",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .map_err(database_error)?;
    let supplied_password = Zeroizing::new(request.password);
    let password_valid = if let Some(user) = user.as_ref() {
        verify_password(supplied_password, user.password_hash.clone()).await
    } else {
        consume_password_work(supplied_password).await;
        false
    };
    let Some(user) = user.filter(|user| password_valid && user.enabled) else {
        record_login_failure(&state.pool, &username_hash, "rejected").await?;
        record_login_audit(&state.pool, &username_hash, "invalid_credentials").await?;
        return Err(ApiError::unauthorized());
    };
    let role = ConsoleRole::parse(&user.role).ok_or_else(ApiError::internal)?;
    Ok((user, role))
}

async fn create_login_session(
    state: &AppState,
    user: LoginUserRow,
    role: ConsoleRole,
) -> Result<(String, LoginResponse), ApiError> {
    let (session_token, token_hash) = random_token(SESSION_TOKEN_DOMAIN)?;
    let (csrf_token, csrf_hash) = random_token(CSRF_TOKEN_DOMAIN)?;
    let expires_at = Utc::now()
        + Duration::seconds(
            i64::try_from(state.console_session_ttl_seconds).map_err(|_| ApiError::internal())?,
        );
    let session_id = Uuid::new_v4();
    let mut transaction = state.pool.begin().await.map_err(database_error)?;
    sqlx::query(
        "UPDATE console_sessions
         SET revoked_at = now()
         WHERE user_id = $1 AND revoked_at IS NULL
           AND id IN (
               SELECT id FROM console_sessions
               WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > now()
               ORDER BY created_at DESC
               OFFSET $2
           )",
    )
    .bind(user.id)
    .bind(MAX_ACTIVE_SESSIONS - 1)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    sqlx::query(
        "INSERT INTO console_sessions
         (id, user_id, token_hash, csrf_hash, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(session_id)
    .bind(user.id)
    .bind(token_hash.as_slice())
    .bind(csrf_hash.as_slice())
    .bind(expires_at)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    sqlx::query("UPDATE console_users SET last_login_at = now(), updated_at = now() WHERE id = $1")
        .bind(user.id)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    append_auth_audit(
        &mut transaction,
        "console_user",
        &user.id.to_string(),
        "auth.login",
        "console_session",
        Some(session_id.to_string()),
        "success",
        serde_json::json!({}),
    )
    .await?;
    transaction.commit().await.map_err(database_error)?;
    let _ = sqlx::query(
        "DELETE FROM console_login_attempts WHERE occurred_at < now() - interval '1 day'",
    )
    .execute(&state.pool)
    .await;
    Ok((
        session_token,
        LoginResponse {
            user: user_response(user, role),
            csrf_token,
            expires_at,
        },
    ))
}

pub(crate) async fn current_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<SessionResponse>), ApiError> {
    let session = authenticate_session(&state, &headers).await?;
    let (csrf_token, csrf_hash) = random_token(CSRF_TOKEN_DOMAIN)?;
    sqlx::query(
        "UPDATE console_sessions SET csrf_hash = $1, last_seen_at = now()
         WHERE id = $2 AND revoked_at IS NULL AND expires_at > now()",
    )
    .bind(csrf_hash.as_slice())
    .bind(session.session_id)
    .execute(&state.pool)
    .await
    .map_err(database_error)?;
    Ok((
        no_store_headers(),
        Json(SessionResponse {
            user: session.user,
            csrf_token,
            expires_at: session.expires_at,
        }),
    ))
}

pub(crate) async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, StatusCode), ApiError> {
    let session = authenticate_session(&state, &headers).await?;
    verify_csrf(&headers, &session.csrf_hash)?;
    let mut transaction = state.pool.begin().await.map_err(database_error)?;
    sqlx::query(
        "UPDATE console_sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(session.session_id)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    append_auth_audit(
        &mut transaction,
        "console_user",
        &session.user.id.to_string(),
        "auth.logout",
        "console_session",
        Some(session.session_id.to_string()),
        "success",
        serde_json::json!({}),
    )
    .await?;
    transaction.commit().await.map_err(database_error)?;
    let mut response_headers = no_store_headers();
    response_headers.insert(
        header::SET_COOKIE,
        expired_session_cookie(state.console_cookie_secure)?,
    );
    Ok((response_headers, StatusCode::NO_CONTENT))
}

pub(crate) async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ConsoleUserResponse>>, ApiError> {
    authorize(&state, &headers, Permission::Read, false).await?;
    let users = sqlx::query_as::<_, LoginUserRow>(
        "SELECT id, username, display_name, password_hash, role, enabled,
                created_at, last_login_at
         FROM console_users
         ORDER BY username",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(database_error)?;
    let users = users
        .into_iter()
        .map(|user| {
            let role = ConsoleRole::parse(&user.role).ok_or_else(ApiError::internal)?;
            Ok(user_response(user, role))
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(Json(users))
}

pub(crate) async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateConsoleUserRequest>,
) -> Result<(StatusCode, Json<ConsoleUserResponse>), ApiError> {
    let actor = authorize(&state, &headers, Permission::ManageUsers, true).await?;
    let username = normalize_username(&request.username).ok_or_else(ApiError::validation)?;
    let display_name = request.display_name.trim().to_owned();
    if !(1..=80).contains(&display_name.chars().count())
        || !(12..=128).contains(&request.password.chars().count())
    {
        return Err(ApiError::validation());
    }
    let role = ConsoleRole::parse(&request.role).ok_or_else(ApiError::validation)?;
    let password_hash = hash_password(Zeroizing::new(request.password)).await?;
    let user_id = Uuid::new_v4();
    let created_at = Utc::now();
    let mut transaction = state.pool.begin().await.map_err(database_error)?;
    let inserted = sqlx::query(
        "INSERT INTO console_users
         (id, username, display_name, password_hash, role, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $6)",
    )
    .bind(user_id)
    .bind(&username)
    .bind(&display_name)
    .bind(password_hash)
    .bind(role.as_str())
    .bind(created_at)
    .execute(&mut *transaction)
    .await;
    if let Err(error) = inserted {
        if error
            .as_database_error()
            .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
        {
            return Err(ApiError::conflict());
        }
        return Err(database_error(error));
    }
    append_auth_audit(
        &mut transaction,
        actor.actor_type(),
        actor.actor_id(),
        "console_user.create",
        "console_user",
        Some(user_id.to_string()),
        "success",
        serde_json::json!({"username": username, "role": role.as_str()}),
    )
    .await?;
    transaction.commit().await.map_err(database_error)?;
    Ok((
        StatusCode::CREATED,
        Json(ConsoleUserResponse {
            id: user_id,
            username,
            display_name,
            role: role.as_str(),
            enabled: true,
            created_at,
            last_login_at: None,
        }),
    ))
}

pub(crate) async fn authorize(
    state: &AppState,
    headers: &HeaderMap,
    permission: Permission,
    require_csrf: bool,
) -> Result<ManagementActor, ApiError> {
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        let token = value
            .to_str()
            .ok()
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or_else(ApiError::unauthorized)?;
        if token.chars().count() < 32 || token.len() > 256 {
            return Err(ApiError::unauthorized());
        }
        let candidate: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        if !bool::from(candidate.ct_eq(&state.admin_token_hash)) {
            return Err(ApiError::unauthorized());
        }
        return Ok(ManagementActor {
            actor_type: "api_token",
            actor_id: "bootstrap-api-token".to_owned(),
        });
    }

    let session = authenticate_session(state, headers).await?;
    if !session.role.permits(permission) {
        return Err(ApiError::forbidden());
    }
    if require_csrf {
        verify_csrf(headers, &session.csrf_hash)?;
    }
    sqlx::query("UPDATE console_sessions SET last_seen_at = now() WHERE id = $1")
        .bind(session.session_id)
        .execute(&state.pool)
        .await
        .map_err(database_error)?;
    Ok(ManagementActor {
        actor_type: "console_user",
        actor_id: session.user.id.to_string(),
    })
}

async fn authenticate_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthenticatedSession, ApiError> {
    let token = session_cookie_value(headers).ok_or_else(ApiError::unauthorized)?;
    if token.len() > 128 {
        return Err(ApiError::unauthorized());
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| ApiError::unauthorized())?;
    if decoded.len() != 32 {
        return Err(ApiError::unauthorized());
    }
    let token_hash = domain_hash(SESSION_TOKEN_DOMAIN, token.as_bytes());
    let row = sqlx::query_as::<_, SessionRow>(
        "SELECT s.id AS session_id, s.csrf_hash, s.expires_at,
                u.id AS user_id, u.username, u.display_name, u.role, u.enabled,
                u.created_at, u.last_login_at
         FROM console_sessions s
         JOIN console_users u ON u.id = s.user_id
         WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > now()
           AND u.enabled",
    )
    .bind(token_hash.as_slice())
    .fetch_optional(&state.pool)
    .await
    .map_err(database_error)?
    .ok_or_else(ApiError::unauthorized)?;
    let csrf_hash: [u8; 32] = row.csrf_hash.try_into().map_err(|_| ApiError::internal())?;
    let role = ConsoleRole::parse(&row.role).ok_or_else(ApiError::internal)?;
    Ok(AuthenticatedSession {
        session_id: row.session_id,
        csrf_hash,
        expires_at: row.expires_at,
        role,
        user: ConsoleUserResponse {
            id: row.user_id,
            username: row.username,
            display_name: row.display_name,
            role: role.as_str(),
            enabled: row.enabled,
            created_at: row.created_at,
            last_login_at: row.last_login_at,
        },
    })
}

fn verify_csrf(headers: &HeaderMap, expected_hash: &[u8; 32]) -> Result<(), ApiError> {
    let token = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .filter(|value| (32..=128).contains(&value.len()))
        .ok_or_else(ApiError::forbidden)?;
    let candidate = domain_hash(CSRF_TOKEN_DOMAIN, token.as_bytes());
    if bool::from(candidate.ct_eq(expected_hash)) {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}

async fn login_failure_count(pool: &PgPool, username_hash: &[u8; 32]) -> Result<i64, ApiError> {
    sqlx::query_scalar(
        "SELECT count(*) FROM console_login_attempts
         WHERE username_hash = $1
           AND occurred_at > now() - make_interval(mins => $2)",
    )
    .bind(username_hash.as_slice())
    .bind(i32::try_from(LOGIN_WINDOW_MINUTES).map_err(|_| ApiError::internal())?)
    .fetch_one(pool)
    .await
    .map_err(database_error)
}

async fn record_login_failure(
    pool: &PgPool,
    username_hash: &[u8; 32],
    outcome: &'static str,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO console_login_attempts (username_hash, outcome) VALUES ($1, $2)")
        .bind(username_hash.as_slice())
        .bind(outcome)
        .execute(pool)
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn record_login_audit(
    pool: &PgPool,
    username_hash: &[u8; 32],
    reason: &'static str,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO audit_events
         (actor_type, actor_id, action, target_type, outcome, metadata)
         VALUES ('anonymous', $1, 'auth.login', 'console_session', 'rejected', $2)",
    )
    .bind(URL_SAFE_NO_PAD.encode(username_hash))
    .bind(serde_json::json!({"reason_class": reason}))
    .execute(pool)
    .await
    .map_err(database_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn append_auth_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor_type: &str,
    actor_id: &str,
    action: &str,
    target_type: &str,
    target_id: Option<String>,
    outcome: &str,
    metadata: serde_json::Value,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO audit_events
         (actor_type, actor_id, action, target_type, target_id, outcome, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(actor_type)
    .bind(actor_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(outcome)
    .bind(metadata)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    Ok(())
}

async fn hash_password(password: Zeroizing<String>) -> Result<String, ApiError> {
    tokio::task::spawn_blocking(move || hash_password_sync(&password))
        .await
        .map_err(|_| ApiError::internal())?
        .map_err(|_| ApiError::internal())
}

async fn hash_password_for_bootstrap(
    password: Zeroizing<String>,
) -> Result<String, BootstrapError> {
    tokio::task::spawn_blocking(move || hash_password_sync(&password))
        .await
        .map_err(|_| BootstrapError::PasswordHash)?
        .map_err(|error| match error {
            PasswordWorkError::Random => BootstrapError::Random,
            PasswordWorkError::Hash => BootstrapError::PasswordHash,
        })
}

#[derive(Clone, Copy)]
enum PasswordWorkError {
    Random,
    Hash,
}

fn hash_password_sync(password: &str) -> Result<String, PasswordWorkError> {
    let mut salt_bytes = [0_u8; 16];
    getrandom::fill(&mut salt_bytes).map_err(|_| PasswordWorkError::Random)?;
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|_| PasswordWorkError::Hash)?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| PasswordWorkError::Hash)
}

async fn verify_password(password: Zeroizing<String>, password_hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&password_hash).ok().is_some_and(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false)
}

async fn consume_password_work(password: Zeroizing<String>) {
    let _ = tokio::task::spawn_blocking(move || {
        let salt = SaltString::encode_b64(&[0x5a; 16]).ok();
        if let Some(salt) = salt {
            let _ = Argon2::default().hash_password(password.as_bytes(), &salt);
        }
    })
    .await;
}

fn random_token(domain: &[u8]) -> Result<(String, [u8; 32]), ApiError> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random).map_err(|_| ApiError::internal())?;
    let token = URL_SAFE_NO_PAD.encode(random);
    let hash = domain_hash(domain, token.as_bytes());
    Ok((token, hash))
}

fn domain_hash(domain: &[u8], value: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(value);
    digest.finalize().into()
}

fn normalize_username(username: &str) -> Option<String> {
    let normalized = username.trim().to_ascii_lowercase();
    let valid = (3..=64).contains(&normalized.len())
        && normalized.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'.' | b'_' | b'-'))
        });
    valid.then_some(normalized)
}

fn user_response(user: LoginUserRow, role: ConsoleRole) -> ConsoleUserResponse {
    ConsoleUserResponse {
        id: user.id,
        username: user.username,
        display_name: user.display_name,
        role: role.as_str(),
        enabled: user.enabled,
        created_at: user.created_at,
        last_login_at: user.last_login_at,
    }
}

fn session_cookie(token: &str, max_age: u64, secure: bool) -> Result<HeaderValue, ApiError> {
    let secure_attribute = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure_attribute}"
    ))
    .map_err(|_| ApiError::internal())
}

fn expired_session_cookie(secure: bool) -> Result<HeaderValue, ApiError> {
    let secure_attribute = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{secure_attribute}"
    ))
    .map_err(|_| ApiError::internal())
}

fn session_cookie_value(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|pair| pair.strip_prefix(&format!("{SESSION_COOKIE}=")))
}

fn no_store_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers
}

fn database_error(_error: sqlx::Error) -> ApiError {
    ApiError::internal()
}
