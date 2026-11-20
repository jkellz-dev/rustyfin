-- How long each library's last scan took (owner, 2026-11-20: "I'd like to see
-- the duration each task took to complete in the admin. Especially scans.").
ALTER TABLE libraries ADD COLUMN IF NOT EXISTS last_scan_duration_ms BIGINT;
