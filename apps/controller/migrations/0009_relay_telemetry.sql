CREATE TABLE relay_telemetry_reports (
    relay_id bytea PRIMARY KEY,
    boot_id bytea NOT NULL,
    sequence bigint NOT NULL,
    report jsonb NOT NULL,
    packets_received bigint NOT NULL,
    bytes_received bigint NOT NULL,
    packets_forwarded bigint NOT NULL,
    bytes_forwarded bigint NOT NULL,
    packets_dropped bigint NOT NULL,
    io_errors bigint NOT NULL,
    forwarding_latency_samples bigint NOT NULL,
    forwarding_latency_microseconds_total bigint NOT NULL,
    generated_at timestamptz NOT NULL,
    received_at timestamptz NOT NULL DEFAULT now(),
    CHECK (octet_length(relay_id) = 16),
    CHECK (octet_length(boot_id) = 16),
    CHECK (sequence > 0),
    CHECK (pg_column_size(report) <= 65536),
    CHECK (packets_received >= 0),
    CHECK (bytes_received >= 0),
    CHECK (packets_forwarded >= 0),
    CHECK (bytes_forwarded >= 0),
    CHECK (packets_dropped >= 0),
    CHECK (io_errors >= 0),
    CHECK (forwarding_latency_samples >= 0),
    CHECK (forwarding_latency_microseconds_total >= 0)
);

CREATE INDEX relay_telemetry_reports_recent_idx
    ON relay_telemetry_reports (received_at DESC);

CREATE TABLE relay_telemetry_samples (
    relay_id bytea NOT NULL,
    boot_id bytea NOT NULL,
    sequence bigint NOT NULL,
    packets_received bigint NOT NULL,
    bytes_received bigint NOT NULL,
    packets_forwarded bigint NOT NULL,
    bytes_forwarded bigint NOT NULL,
    packets_dropped bigint NOT NULL,
    io_errors bigint NOT NULL,
    forwarding_latency_samples bigint NOT NULL,
    forwarding_latency_microseconds_total bigint NOT NULL,
    generated_at timestamptz NOT NULL,
    received_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (relay_id, boot_id, sequence),
    CHECK (octet_length(relay_id) = 16),
    CHECK (octet_length(boot_id) = 16),
    CHECK (sequence > 0),
    CHECK (packets_received >= 0),
    CHECK (bytes_received >= 0),
    CHECK (packets_forwarded >= 0),
    CHECK (bytes_forwarded >= 0),
    CHECK (packets_dropped >= 0),
    CHECK (io_errors >= 0),
    CHECK (forwarding_latency_samples >= 0),
    CHECK (forwarding_latency_microseconds_total >= 0)
);

CREATE INDEX relay_telemetry_samples_window_idx
    ON relay_telemetry_samples (received_at DESC, relay_id);
