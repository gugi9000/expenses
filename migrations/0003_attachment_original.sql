-- Set when the stored file is an auto-cropped version; the untouched upload is kept for reference.
ALTER TABLE attachments ADD COLUMN original_sha256 TEXT;
