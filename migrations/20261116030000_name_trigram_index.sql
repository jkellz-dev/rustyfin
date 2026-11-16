-- SearchTerm on /Items is `name ILIKE '%term%'`, which no b-tree index serves:
-- 94 ms per search over 81,000 items, ~2 s under load (11d stress run,
-- 2026-11-16). A trigram index makes it ~0.5 ms. pg_trgm ships with
-- Postgres and is a trusted extension (13+), so the database owner may
-- create it; where that is not allowed the search keeps working, unindexed.
DO $$
BEGIN
    -- In public, whatever the search_path: the tests migrate one schema per
    -- test, and an extension created in one of those would vanish with it.
    CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;
EXCEPTION
    WHEN insufficient_privilege OR undefined_file THEN
        RAISE NOTICE 'pg_trgm not available (%), name search stays unindexed', SQLERRM;
END
$$;

-- The operator class is named with the extension's schema, which need not be
-- on the search_path.
DO $$
DECLARE
    ext_schema text;
BEGIN
    SELECT n.nspname INTO ext_schema
    FROM pg_extension e JOIN pg_namespace n ON n.oid = e.extnamespace
    WHERE e.extname = 'pg_trgm';
    IF ext_schema IS NOT NULL THEN
        EXECUTE format(
            'CREATE INDEX IF NOT EXISTS idx_media_items_name_trgm '
            'ON media_items USING gin (name %I.gin_trgm_ops)',
            ext_schema
        );
    END IF;
END
$$;
