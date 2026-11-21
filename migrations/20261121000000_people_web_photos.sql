-- People whose stored photo is a local path from another program (Jellyfin-era
-- NFOs pointed at /config/metadata/People/...) never got the TMDB address the
-- *arr NFOs give now: the address was kept from the first title read. The
-- server now lets a web address replace a local path; this makes the next scan
-- read the films and episodes those people are in again (series are read on
-- every scan), so their rows pick it up. One slower scan; nothing is deleted.
UPDATE media_items
SET etag = NULL
WHERE item_type IN ('Movie', 'Episode')
  AND etag IS NOT NULL
  AND id IN (
    SELECT mip.item_id
    FROM media_item_people mip
    JOIN people p ON p.id = mip.person_id
    WHERE p.image_url IS NOT NULL
      AND p.image_url !~* '^https?://'
  );
