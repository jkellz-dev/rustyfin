-- Add optional scan schedule override per library.
-- NULL means "use config default". Value references a named schedule key.
ALTER TABLE libraries ADD COLUMN scan_schedule TEXT;
