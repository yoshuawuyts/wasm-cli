-- Add UNIQUE constraint to prevent duplicate image entries
-- Use COALESCE to handle NULL values in unique index (SQLite treats NULLs as distinct)
CREATE UNIQUE INDEX IF NOT EXISTS idx_image_unique ON image(
    ref_registry, 
    ref_repository, 
    COALESCE(ref_tag, ''), 
    COALESCE(ref_digest, '')
);
