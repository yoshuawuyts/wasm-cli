-- Sync metadata table for tracking registry sync state.
-- Stores key-value pairs like ETag headers and last sync timestamps.
CREATE TABLE IF NOT EXISTS _sync_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
