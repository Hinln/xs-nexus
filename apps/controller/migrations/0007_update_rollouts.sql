CREATE TABLE update_releases (
    id uuid PRIMARY KEY,
    version text NOT NULL,
    platform text NOT NULL,
    architecture text NOT NULL,
    target text NOT NULL,
    archive_name text NOT NULL,
    archive_size bigint NOT NULL,
    archive_sha256 bytea NOT NULL,
    archive_url text NOT NULL,
    manifest bytea NOT NULL,
    manifest_sha256 bytea NOT NULL UNIQUE,
    signature bytea NOT NULL,
    created_by_type text NOT NULL,
    created_by_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (version ~ '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'),
    CHECK (platform = 'linux'),
    CHECK (architecture IN ('x86_64', 'aarch64')),
    CHECK (target IN ('x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu')),
    CHECK (archive_size BETWEEN 1 AND 536870912),
    CHECK (octet_length(archive_sha256) = 32),
    CHECK (octet_length(manifest) BETWEEN 1 AND 4096),
    CHECK (octet_length(manifest_sha256) = 32),
    CHECK (octet_length(signature) = 64),
    CHECK (char_length(archive_url) BETWEEN 1 AND 2048),
    CHECK (created_by_type IN ('api_token', 'console_user')),
    CHECK (char_length(created_by_id) BETWEEN 1 AND 128),
    UNIQUE (version, platform, architecture)
);

CREATE TABLE update_rollout_policies (
    network_id uuid NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
    channel text NOT NULL,
    platform text NOT NULL,
    architecture text NOT NULL,
    release_id uuid NOT NULL REFERENCES update_releases(id) ON DELETE RESTRICT,
    minimum_version text,
    rollout_basis_points integer NOT NULL,
    paused boolean NOT NULL,
    generation bigint NOT NULL,
    updated_by_type text NOT NULL,
    updated_by_id text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (network_id, channel, platform, architecture),
    CHECK (channel IN ('stable', 'testing', 'development')),
    CHECK (platform = 'linux'),
    CHECK (architecture IN ('x86_64', 'aarch64')),
    CHECK (minimum_version IS NULL OR minimum_version ~ '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'),
    CHECK (rollout_basis_points BETWEEN 0 AND 10000),
    CHECK (generation > 0),
    CHECK (updated_by_type IN ('api_token', 'console_user')),
    CHECK (char_length(updated_by_id) BETWEEN 1 AND 128)
);

CREATE INDEX update_rollout_release_idx ON update_rollout_policies (release_id);

ALTER TABLE nodes
    ADD COLUMN update_channel text NOT NULL DEFAULT 'stable',
    ADD CONSTRAINT nodes_update_channel_valid
        CHECK (update_channel IN ('stable', 'testing', 'development'));

CREATE TABLE node_update_reports (
    node_id bytea PRIMARY KEY REFERENCES nodes(node_id) ON DELETE CASCADE,
    agent_version text NOT NULL,
    platform text NOT NULL,
    architecture text NOT NULL,
    update_channel text NOT NULL,
    update_state text NOT NULL,
    observed_release_id uuid REFERENCES update_releases(id) ON DELETE SET NULL,
    last_error_code text,
    generated_at timestamptz NOT NULL,
    received_at timestamptz NOT NULL DEFAULT now(),
    CHECK (octet_length(node_id) = 16),
    CHECK (agent_version ~ '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'),
    CHECK (platform IN ('linux', 'windows')),
    CHECK (architecture IN ('x86_64', 'aarch64')),
    CHECK (update_channel IN ('stable', 'testing', 'development')),
    CHECK (update_state IN ('idle', 'downloading', 'staged', 'applying', 'failed')),
    CHECK (last_error_code IS NULL OR last_error_code ~ '^[a-z][a-z0-9_]{0,63}$')
);

CREATE INDEX node_update_reports_recent_idx ON node_update_reports (received_at DESC);
