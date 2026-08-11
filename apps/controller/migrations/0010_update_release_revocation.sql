ALTER TABLE update_releases
    ADD COLUMN revoked_at timestamptz,
    ADD COLUMN revoked_by_type text,
    ADD COLUMN revoked_by_id text,
    ADD COLUMN revocation_reason text,
    ADD CONSTRAINT update_releases_revocation_complete CHECK (
        (revoked_at IS NULL AND revoked_by_type IS NULL AND revoked_by_id IS NULL AND revocation_reason IS NULL)
        OR
        (revoked_at IS NOT NULL AND revoked_by_type IS NOT NULL AND revoked_by_id IS NOT NULL AND revocation_reason IS NOT NULL)
    ),
    ADD CONSTRAINT update_releases_revoked_by_type_valid CHECK (
        revoked_by_type IS NULL OR revoked_by_type IN ('api_token', 'console_user')
    ),
    ADD CONSTRAINT update_releases_revoked_by_id_valid CHECK (
        revoked_by_id IS NULL OR char_length(revoked_by_id) BETWEEN 1 AND 128
    ),
    ADD CONSTRAINT update_releases_revocation_reason_valid CHECK (
        revocation_reason IS NULL OR revocation_reason IN (
            'build_error',
            'key_compromise',
            'security_issue',
            'superseded',
            'withdrawn'
        )
    );

CREATE INDEX update_releases_active_idx
    ON update_releases (platform, architecture, created_at DESC)
    WHERE revoked_at IS NULL;
