-- Per-service logical databases (ADR-003: database-per-service).
-- Runs only on first boot against an empty pg_data volume.
CREATE DATABASE auth_db;
CREATE DATABASE movie_db;
CREATE DATABASE song_db;
CREATE DATABASE license_db;
CREATE DATABASE notification_db;
