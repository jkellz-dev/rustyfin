-- A friend's library with no local counterpart gets a library of its own
-- (owner, 2026-11-18): no folders, filled only by its link. Removing the link
-- removes the library and what is in it.
ALTER TABLE libraries
    ADD COLUMN IF NOT EXISTS link_id UUID REFERENCES links (id) ON DELETE CASCADE;
