ALTER TABLE networks
    ADD COLUMN policy_version bigint NOT NULL DEFAULT 1,
    ADD CONSTRAINT networks_policy_version_check CHECK (policy_version > 0);

ALTER TABLE nodes
    ADD CONSTRAINT nodes_network_id_id_unique UNIQUE (network_id, id);

CREATE TABLE node_groups (
    network_id uuid NOT NULL REFERENCES networks(id) ON DELETE RESTRICT,
    name text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (network_id, name),
    CHECK (name ~ '^[a-z0-9][a-z0-9._-]{0,63}$')
);

CREATE TABLE node_group_memberships (
    network_id uuid NOT NULL,
    group_name text NOT NULL,
    node_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (network_id, group_name, node_id),
    FOREIGN KEY (network_id, group_name)
        REFERENCES node_groups(network_id, name) ON DELETE CASCADE,
    FOREIGN KEY (network_id, node_id)
        REFERENCES nodes(network_id, id) ON DELETE RESTRICT
);

CREATE INDEX node_group_memberships_node_idx
    ON node_group_memberships (network_id, node_id);

CREATE TABLE acl_rules (
    network_id uuid NOT NULL REFERENCES networks(id) ON DELETE RESTRICT,
    rule_id text NOT NULL,
    priority bigint NOT NULL,
    action text NOT NULL,
    protocol text NOT NULL,
    sources jsonb NOT NULL,
    destinations jsonb NOT NULL,
    destination_ports jsonb NOT NULL DEFAULT '[]',
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (network_id, rule_id),
    CHECK (rule_id ~ '^[a-z0-9][a-z0-9._-]{0,63}$'),
    CHECK (priority BETWEEN 1 AND 4294967295),
    CHECK (action IN ('allow', 'deny')),
    CHECK (protocol IN ('any', 'tcp', 'udp', 'icmp')),
    CHECK (jsonb_typeof(sources) = 'array'),
    CHECK (jsonb_array_length(sources) BETWEEN 1 AND 32),
    CHECK (jsonb_typeof(destinations) = 'array'),
    CHECK (jsonb_array_length(destinations) BETWEEN 1 AND 32),
    CHECK (jsonb_typeof(destination_ports) = 'array'),
    CHECK (jsonb_array_length(destination_ports) <= 64)
);

CREATE INDEX acl_rules_network_order_idx
    ON acl_rules (network_id, priority DESC, rule_id);
