CREATE TABLE node_telemetry_reports (
    node_id bytea PRIMARY KEY REFERENCES nodes(node_id) ON DELETE CASCADE,
    network_id uuid NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
    boot_id bytea NOT NULL,
    sequence bigint NOT NULL,
    report jsonb NOT NULL,
    tx_bytes_total bigint NOT NULL,
    rx_bytes_total bigint NOT NULL,
    handshake_attempts_total bigint NOT NULL,
    handshake_successes_total bigint NOT NULL,
    latency_samples_total bigint NOT NULL,
    latency_microseconds_total bigint NOT NULL,
    generated_at timestamptz NOT NULL,
    received_at timestamptz NOT NULL DEFAULT now(),
    CHECK (octet_length(node_id) = 16),
    CHECK (octet_length(boot_id) = 16),
    CHECK (sequence > 0),
    CHECK (pg_column_size(report) <= 524288),
    CHECK (tx_bytes_total >= 0),
    CHECK (rx_bytes_total >= 0),
    CHECK (handshake_attempts_total >= 0),
    CHECK (handshake_successes_total >= 0),
    CHECK (handshake_successes_total <= handshake_attempts_total),
    CHECK (latency_samples_total >= 0),
    CHECK (latency_microseconds_total >= 0)
);

CREATE INDEX node_telemetry_reports_network_idx
    ON node_telemetry_reports (network_id, received_at DESC);

CREATE TABLE node_telemetry_samples (
    node_id bytea NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
    network_id uuid NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
    boot_id bytea NOT NULL,
    sequence bigint NOT NULL,
    tx_bytes_total bigint NOT NULL,
    rx_bytes_total bigint NOT NULL,
    handshake_attempts_total bigint NOT NULL,
    handshake_successes_total bigint NOT NULL,
    latency_samples_total bigint NOT NULL,
    latency_microseconds_total bigint NOT NULL,
    generated_at timestamptz NOT NULL,
    received_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (node_id, boot_id, sequence),
    CHECK (octet_length(node_id) = 16),
    CHECK (octet_length(boot_id) = 16),
    CHECK (sequence > 0),
    CHECK (tx_bytes_total >= 0),
    CHECK (rx_bytes_total >= 0),
    CHECK (handshake_attempts_total >= 0),
    CHECK (handshake_successes_total >= 0),
    CHECK (handshake_successes_total <= handshake_attempts_total),
    CHECK (latency_samples_total >= 0),
    CHECK (latency_microseconds_total >= 0)
);

CREATE INDEX node_telemetry_samples_window_idx
    ON node_telemetry_samples (received_at DESC, network_id, node_id);
