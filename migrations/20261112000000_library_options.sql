-- Per-library options as sent by Jellyfin clients (LibraryOptions). Stored as the
-- client's JSON so fields we do not interpret still round-trip.
ALTER TABLE libraries ADD COLUMN library_options JSONB NOT NULL DEFAULT '{}'::jsonb;
