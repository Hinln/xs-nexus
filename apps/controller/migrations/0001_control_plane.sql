CREATE SEQUENCE credential_serial AS bigint START WITH 1 INCREMENT BY 1 NO CYCLE;

CREATE TABLE networks (
    id uuid PRIMARY KEY,
    name text NOT NULL,
    address_pool cidr NOT NULL,
    reserved_addresses integer NOT NULL DEFAULT 16,
    config_version bigint NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (family(address_pool) = 4),
    CHECK (masklen(address_pool) BETWEEN 8 AND 30),
    CHECK (reserved_addresses BETWEEN 2 AND 4096),
    CHECK (config_version >= 0)
);

CREATE UNIQUE INDEX networks_name_unique ON networks (lower(name));

CREATE TABLE enrollment_tokens (
    id uuid PRIMARY KEY,
    network_id uuid NOT NULL REFERENCES networks(id) ON DELETE RESTRICT,
    token_hash bytea NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    max_uses integer NOT NULL,
    use_count integer NOT NULL DEFAULT 0,
    default_role_bitmap bigint NOT NULL DEFAULT 0,
    default_tags text[] NOT NULL DEFAULT '{}',
    requested_virtual_ip inet,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    created_by text NOT NULL,
    CHECK (octet_length(token_hash) = 32),
    CHECK (max_uses BETWEEN 1 AND 100),
    CHECK (use_count BETWEEN 0 AND max_uses),
    CHECK (default_role_bitmap BETWEEN 0 AND 4294967295),
    CHECK (requested_virtual_ip IS NULL OR family(requested_virtual_ip) = 4)
);

CREATE INDEX enrollment_tokens_network_id_idx ON enrollment_tokens (network_id);

CREATE TABLE nodes (
    id uuid PRIMARY KEY,
    network_id uuid NOT NULL REFERENCES networks(id) ON DELETE RESTRICT,
    node_id bytea NOT NULL,
    name text NOT NULL,
    device_type text NOT NULL,
    identity_public_key bytea NOT NULL,
    virtual_ip inet NOT NULL,
    role_bitmap bigint NOT NULL,
    tags text[] NOT NULL DEFAULT '{}',
    credential bytea NOT NULL,
    credential_serial bigint NOT NULL UNIQUE,
    credential_key_id bigint NOT NULL,
    credential_not_before timestamptz NOT NULL,
    credential_not_after timestamptz NOT NULL,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (octet_length(node_id) = 16),
    CHECK (octet_length(identity_public_key) = 32),
    CHECK (octet_length(credential) = 200),
    CHECK (family(virtual_ip) = 4),
    CHECK (role_bitmap BETWEEN 0 AND 4294967295),
    CHECK (credential_key_id BETWEEN 0 AND 4294967295),
    CHECK (credential_not_after > credential_not_before)
);

CREATE UNIQUE INDEX nodes_node_id_unique ON nodes (node_id);
CREATE UNIQUE INDEX nodes_identity_public_key_unique ON nodes (identity_public_key);
CREATE UNIQUE INDEX nodes_network_virtual_ip_active_unique
    ON nodes (network_id, virtual_ip)
    WHERE revoked_at IS NULL;
CREATE INDEX nodes_network_id_idx ON nodes (network_id);

CREATE TABLE ip_leases (
    network_id uuid NOT NULL REFERENCES networks(id) ON DELETE RESTRICT,
    virtual_ip inet NOT NULL,
    node_id uuid NOT NULL UNIQUE REFERENCES nodes(id) ON DELETE RESTRICT,
    state text NOT NULL,
    cooldown_until timestamptz,
    allocated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (network_id, virtual_ip),
    CHECK (family(virtual_ip) = 4),
    CHECK (state IN ('active', 'cooling')),
    CHECK (
        (state = 'active' AND cooldown_until IS NULL)
        OR (state = 'cooling' AND cooldown_until IS NOT NULL)
    )
);

CREATE TABLE configuration_versions (
    network_id uuid NOT NULL REFERENCES networks(id) ON DELETE RESTRICT,
    version bigint NOT NULL,
    payload bytea NOT NULL,
    signature bytea NOT NULL,
    signer_key_id bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (network_id, version),
    CHECK (version > 0),
    CHECK (octet_length(payload) BETWEEN 2 AND 1048576),
    CHECK (octet_length(signature) = 64),
    CHECK (signer_key_id BETWEEN 0 AND 4294967295)
);

CREATE TABLE audit_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    occurred_at timestamptz NOT NULL DEFAULT now(),
    network_id uuid REFERENCES networks(id) ON DELETE RESTRICT,
    actor_type text NOT NULL,
    actor_id text NOT NULL,
    action text NOT NULL,
    target_type text NOT NULL,
    target_id text,
    outcome text NOT NULL,
    metadata jsonb NOT NULL DEFAULT '{}',
    CHECK (outcome IN ('success', 'rejected', 'failure')),
    CHECK (jsonb_typeof(metadata) = 'object')
);

CREATE INDEX audit_events_occurred_at_idx ON audit_events (occurred_at DESC);
CREATE INDEX audit_events_network_id_idx ON audit_events (network_id, occurred_at DESC);

CREATE FUNCTION reject_audit_event_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'audit events are append-only';
END;
$$;

CREATE TRIGGER audit_events_no_update
BEFORE UPDATE OR DELETE ON audit_events
FOR EACH ROW EXECUTE FUNCTION reject_audit_event_mutation();
