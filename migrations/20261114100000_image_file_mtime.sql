-- The artwork file's modification time (ms since the epoch) when the row was
-- written. A rescan keeps rows whose files are unchanged, so image tags (the row
-- ids) stay the same and clients can cache images for good; a changed file gets
-- a new row and so a new tag. NULL for rows written before this existed: they
-- are replaced once by the next scan.
ALTER TABLE images ADD COLUMN file_mtime BIGINT;
