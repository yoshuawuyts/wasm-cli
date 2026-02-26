-- Add UNIQUE constraint on wit_text for content-addressable storage
CREATE UNIQUE INDEX IF NOT EXISTS idx_wit_interface_wit_text ON wit_interface(wit_text);
