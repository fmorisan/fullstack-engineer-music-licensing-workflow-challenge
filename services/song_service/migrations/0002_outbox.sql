-- Transactional outbox (ADR-004, platform contract).
CREATE TABLE outbox (
    id UUID PRIMARY KEY,
    topic TEXT NOT NULL,
    key TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at TIMESTAMPTZ
);

CREATE INDEX outbox_unpublished_idx ON outbox (id) WHERE published_at IS NULL;
