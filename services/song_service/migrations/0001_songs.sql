-- song_service initial schema (ADR-003/009).

CREATE TABLE songs (
    id UUID PRIMARY KEY,
    label_id UUID NOT NULL,
    title TEXT NOT NULL,
    author TEXT NOT NULL,
    length_seconds INT NOT NULL CHECK (length_seconds > 0),
    box_art_key TEXT,
    audio_preview_key TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX songs_label_id_idx ON songs (label_id);
