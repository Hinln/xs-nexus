ALTER TABLE nodes
    ADD COLUMN last_control_connected_at timestamptz,
    ADD COLUMN last_control_disconnected_at timestamptz;

CREATE INDEX nodes_last_control_connected_idx
    ON nodes (last_control_connected_at DESC)
    WHERE revoked_at IS NULL;
