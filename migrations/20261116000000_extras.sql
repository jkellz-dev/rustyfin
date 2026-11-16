-- Extras (roadmap 11j): trailers, featurettes and the like belong to a film,
-- show or season. `owner_id` is that title; `extra_type` is Jellyfin's ExtraType.
-- Rows with an owner are never listed as titles of their own.
ALTER TABLE media_items
    ADD COLUMN extra_type TEXT,
    ADD COLUMN owner_id UUID REFERENCES media_items (id) ON DELETE CASCADE;

CREATE INDEX idx_media_items_owner ON media_items (owner_id) WHERE owner_id IS NOT NULL;
