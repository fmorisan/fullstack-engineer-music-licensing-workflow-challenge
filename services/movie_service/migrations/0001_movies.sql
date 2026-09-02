-- movie_service initial schema (ADR-003/009).

CREATE TABLE movies (
    id UUID PRIMARY KEY,
    studio_id UUID NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    poster_key TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX movies_studio_id_idx ON movies (studio_id);
