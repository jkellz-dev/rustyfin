-- A scan looks people up by name and kind (cast and crew from NFOs): 10 ms
-- each over the whole table, about 1,900 lookups a night (owner, 2026-11-24,
-- pg_stat_statements).
CREATE INDEX IF NOT EXISTS idx_people_name_type ON people (name, person_type);
