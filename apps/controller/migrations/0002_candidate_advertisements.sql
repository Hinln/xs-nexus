CREATE TABLE node_candidate_advertisements (
    node_id uuid PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
    generation bigint NOT NULL,
    payload bytea NOT NULL,
    signature bytea NOT NULL,
    expires_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (generation > 0),
    CHECK (octet_length(payload) BETWEEN 2 AND 16384),
    CHECK (octet_length(signature) = 64)
);

CREATE INDEX node_candidate_advertisements_expires_at_idx
    ON node_candidate_advertisements (expires_at);
