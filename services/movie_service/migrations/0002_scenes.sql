-- Scenes with temporal overlap protection (ADR-003: the invariant lives in
-- the database, not application code).
CREATE EXTENSION IF NOT EXISTS btree_gist;

CREATE TABLE scenes (
    movie_id UUID NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
    scene_number INT NOT NULL,
    screen_time_seconds INT NOT NULL CHECK (screen_time_seconds > 0),
    start_time_seconds INT NOT NULL CHECK (start_time_seconds >= 0),
    end_time_seconds INT NOT NULL CHECK (end_time_seconds > start_time_seconds),
    description TEXT NOT NULL DEFAULT '',
    capture_key TEXT,
    PRIMARY KEY (movie_id, scene_number),
    -- Two scenes of the same movie may never share a second of film time;
    -- adjacency (a.end == b.start) is allowed via the half-open int4range.
    EXCLUDE USING gist (
        movie_id WITH =,
        int4range(start_time_seconds, end_time_seconds) WITH &&
    )
);
