ALTER TABLE node_telemetry_reports
    ADD COLUMN acl_drops_total bigint NOT NULL DEFAULT 0 CHECK (acl_drops_total >= 0),
    ADD COLUMN replay_drops_total bigint NOT NULL DEFAULT 0 CHECK (replay_drops_total >= 0);

ALTER TABLE node_telemetry_samples
    ADD COLUMN acl_drops_total bigint NOT NULL DEFAULT 0 CHECK (acl_drops_total >= 0),
    ADD COLUMN replay_drops_total bigint NOT NULL DEFAULT 0 CHECK (replay_drops_total >= 0);

ALTER TABLE relay_telemetry_reports
    ADD COLUMN registration_retries bigint NOT NULL DEFAULT 0 CHECK (registration_retries >= 0),
    ADD COLUMN registrations_rejected bigint NOT NULL DEFAULT 0 CHECK (registrations_rejected >= 0),
    ADD COLUMN invalid_drops bigint NOT NULL DEFAULT 0 CHECK (invalid_drops >= 0),
    ADD COLUMN authentication_drops bigint NOT NULL DEFAULT 0 CHECK (authentication_drops >= 0),
    ADD COLUMN replay_drops bigint NOT NULL DEFAULT 0 CHECK (replay_drops >= 0),
    ADD COLUMN rate_limit_drops bigint NOT NULL DEFAULT 0 CHECK (rate_limit_drops >= 0),
    ADD COLUMN queue_drops bigint NOT NULL DEFAULT 0 CHECK (queue_drops >= 0),
    ADD COLUMN destination_drops bigint NOT NULL DEFAULT 0 CHECK (destination_drops >= 0),
    ADD COLUMN send_drops bigint NOT NULL DEFAULT 0 CHECK (send_drops >= 0);

ALTER TABLE relay_telemetry_samples
    ADD COLUMN registration_retries bigint NOT NULL DEFAULT 0 CHECK (registration_retries >= 0),
    ADD COLUMN registrations_rejected bigint NOT NULL DEFAULT 0 CHECK (registrations_rejected >= 0),
    ADD COLUMN invalid_drops bigint NOT NULL DEFAULT 0 CHECK (invalid_drops >= 0),
    ADD COLUMN authentication_drops bigint NOT NULL DEFAULT 0 CHECK (authentication_drops >= 0),
    ADD COLUMN replay_drops bigint NOT NULL DEFAULT 0 CHECK (replay_drops >= 0),
    ADD COLUMN rate_limit_drops bigint NOT NULL DEFAULT 0 CHECK (rate_limit_drops >= 0),
    ADD COLUMN queue_drops bigint NOT NULL DEFAULT 0 CHECK (queue_drops >= 0),
    ADD COLUMN destination_drops bigint NOT NULL DEFAULT 0 CHECK (destination_drops >= 0),
    ADD COLUMN send_drops bigint NOT NULL DEFAULT 0 CHECK (send_drops >= 0);
