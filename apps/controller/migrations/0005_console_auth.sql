CREATE TABLE console_users (
    id uuid PRIMARY KEY,
    username text NOT NULL,
    display_name text NOT NULL,
    password_hash text NOT NULL,
    role text NOT NULL,
    enabled boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    last_login_at timestamptz,
    CHECK (username ~ '^[a-z0-9][a-z0-9._-]{2,63}$'),
    CHECK (char_length(display_name) BETWEEN 1 AND 80),
    CHECK (char_length(password_hash) BETWEEN 32 AND 512),
    CHECK (role IN ('administrator', 'operator', 'auditor'))
);

CREATE UNIQUE INDEX console_users_username_unique ON console_users (username);

CREATE TABLE console_sessions (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES console_users(id) ON DELETE RESTRICT,
    token_hash bytea NOT NULL UNIQUE,
    csrf_hash bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    CHECK (octet_length(token_hash) = 32),
    CHECK (octet_length(csrf_hash) = 32),
    CHECK (expires_at > created_at)
);

CREATE INDEX console_sessions_user_active_idx
    ON console_sessions (user_id, expires_at DESC)
    WHERE revoked_at IS NULL;

CREATE TABLE console_login_attempts (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username_hash bytea NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT now(),
    outcome text NOT NULL,
    CHECK (octet_length(username_hash) = 32),
    CHECK (outcome IN ('rejected', 'rate_limited'))
);

CREATE INDEX console_login_attempts_window_idx
    ON console_login_attempts (username_hash, occurred_at DESC);
