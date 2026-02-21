-- Phase 11g: Entity images, display preferences, task results

-- 1. Rename item_id → entity_id in images table (unifies media items, persons, studios, artists)
ALTER TABLE images RENAME COLUMN item_id TO entity_id;

-- Drop the FK constraint (entity_id now holds deterministic UUIDs for non-media entities)
ALTER TABLE images DROP CONSTRAINT IF EXISTS images_item_id_fkey;

-- Update CHECK constraint
ALTER TABLE images DROP CONSTRAINT IF EXISTS images_check;
ALTER TABLE images ADD CONSTRAINT images_check
    CHECK (entity_id IS NOT NULL OR user_id IS NOT NULL);

-- Index for entity image lookups
CREATE INDEX IF NOT EXISTS idx_images_entity_id ON images(entity_id) WHERE entity_id IS NOT NULL;

-- 2. Display preferences persistence
CREATE TABLE IF NOT EXISTS display_preferences (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    prefs_id TEXT NOT NULL,
    client TEXT NOT NULL,
    data JSONB NOT NULL,
    PRIMARY KEY (user_id, prefs_id, client)
);

-- 3. Task results for scheduled task history
CREATE TABLE IF NOT EXISTS task_results (
    id BIGSERIAL PRIMARY KEY,
    task_key TEXT NOT NULL,
    task_name TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'Running',
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_task_results_task_key ON task_results(task_key);
CREATE INDEX IF NOT EXISTS idx_task_results_started_at ON task_results(started_at);
