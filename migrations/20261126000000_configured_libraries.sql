-- Libraries set in rustyfin.toml ([[library]]; owner, 2026-11-26: "my goal
-- with configuration is to have it completely config file driven"). The
-- server marks them at start; they are changed only in the file.
ALTER TABLE libraries ADD COLUMN IF NOT EXISTS configured BOOLEAN NOT NULL DEFAULT false;
