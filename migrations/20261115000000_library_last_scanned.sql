-- When each library's last scan finished (the dashboard's library cards).
ALTER TABLE libraries ADD COLUMN IF NOT EXISTS last_scanned_at TIMESTAMPTZ;
