CREATE TABLE node_subnet_route_advertisements (
    node_id uuid PRIMARY KEY REFERENCES nodes(id) ON DELETE RESTRICT,
    network_id uuid NOT NULL REFERENCES networks(id) ON DELETE RESTRICT,
    generation bigint NOT NULL,
    payload bytea NOT NULL,
    signature bytea NOT NULL,
    expires_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (network_id, node_id)
        REFERENCES nodes(network_id, id) ON DELETE RESTRICT,
    CHECK (generation > 0),
    CHECK (octet_length(payload) BETWEEN 2 AND 65536),
    CHECK (octet_length(signature) = 64)
);

CREATE INDEX node_subnet_route_advertisements_network_idx
    ON node_subnet_route_advertisements (network_id, expires_at DESC);

CREATE TABLE subnet_routes (
    network_id uuid NOT NULL REFERENCES networks(id) ON DELETE RESTRICT,
    route_id text NOT NULL,
    gateway_node_id uuid NOT NULL,
    prefix cidr NOT NULL,
    interface_name text NOT NULL,
    mode text NOT NULL,
    priority bigint NOT NULL,
    state text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (network_id, route_id),
    FOREIGN KEY (network_id, gateway_node_id)
        REFERENCES nodes(network_id, id) ON DELETE RESTRICT,
    CHECK (route_id ~ '^[a-z0-9][a-z0-9._-]{0,63}$'),
    CHECK (family(prefix) = 4),
    CHECK (masklen(prefix) BETWEEN 1 AND 30),
    CHECK (interface_name ~ '^[A-Za-z0-9_.-]{1,15}$'),
    CHECK (mode IN ('routed', 'nat')),
    CHECK (priority BETWEEN 1 AND 4294967295),
    CHECK (state IN ('enabled', 'paused', 'revoked'))
);

CREATE UNIQUE INDEX subnet_routes_gateway_scope_active_unique
    ON subnet_routes (network_id, gateway_node_id, prefix, interface_name)
    WHERE state != 'revoked';

CREATE INDEX subnet_routes_configuration_order_idx
    ON subnet_routes (network_id, prefix, priority DESC, route_id)
    WHERE state = 'enabled';
