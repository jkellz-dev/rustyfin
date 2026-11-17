-- What an app saves with POST /Users/{userId}/Configuration beyond the
-- settings that have their own columns (ordered views, excludes, the
-- remember-selection switches, ...), kept as the app sent it.
ALTER TABLE users ADD COLUMN IF NOT EXISTS configuration JSONB;
