-- license_service initial schema (ADR-003/004/009).

CREATE TABLE licenses (
    id UUID PRIMARY KEY,
    movie_id UUID NOT NULL,
    scene_number INT NOT NULL,
    song_id UUID NOT NULL,
    studio_id UUID NOT NULL,
    label_id UUID NOT NULL,
    studio_user_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('OFFER', 'COUNTER_OFFER', 'ACCEPTED', 'REJECTED')),
    license_fee_cents BIGINT NOT NULL CHECK (license_fee_cents >= 0),
    start_time_seconds INT NOT NULL CHECK (start_time_seconds >= 0),
    end_time_seconds INT NOT NULL CHECK (end_time_seconds > start_time_seconds),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX licenses_movie_scene_idx ON licenses (movie_id, scene_number);
CREATE INDEX licenses_song_idx ON licenses (song_id);
CREATE INDEX licenses_label_idx ON licenses (label_id);
CREATE INDEX licenses_studio_idx ON licenses (studio_id);

-- Append-only lifecycle log (00-design.md).
CREATE TABLE license_log (
    id UUID PRIMARY KEY,
    license_id UUID NOT NULL REFERENCES licenses (id) ON DELETE CASCADE,
    sequence INT NOT NULL,
    from_state TEXT,
    to_state TEXT NOT NULL,
    action TEXT NOT NULL,
    actor_user_id UUID NOT NULL,
    license_fee_cents BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (license_id, sequence)
);

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
