-- The stream details Jellyfin clients read from MediaStreams and Rustyfin did not
-- keep: codec profile and level, the container's codec tag, the time base, the
-- reference-frame count, the NAL length size, and whether the picture is
-- interlaced. All nullable except the flag; rows written before this migration
-- simply have no values until the next scan.
ALTER TABLE media_streams
    ADD COLUMN profile TEXT,
    ADD COLUMN level INT,
    ADD COLUMN codec_tag TEXT,
    ADD COLUMN time_base TEXT,
    ADD COLUMN ref_frames INT,
    ADD COLUMN nal_length_size TEXT,
    ADD COLUMN is_interlaced BOOLEAN NOT NULL DEFAULT false;
