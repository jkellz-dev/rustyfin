-- Whether a track has lyrics (a sidecar .lrc/.elrc/.txt or tagged lyrics),
-- recorded by the scan for `HasLyrics` (roadmap 11k). NULL: not yet looked at.
ALTER TABLE media_items ADD COLUMN has_lyrics BOOLEAN;
