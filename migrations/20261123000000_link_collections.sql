-- A linked server's collections (roadmap 16b.4): the name and TMDB
-- collection id by which the friend's films join this server's collections
-- (a film says its own TMDB collection id), and the films of the friend's
-- collections that have no TMDB id. Replaced at each sync of the link.
CREATE TABLE IF NOT EXISTS link_collections (
    link_id UUID NOT NULL REFERENCES links (id) ON DELETE CASCADE,
    remote_id TEXT NOT NULL,
    name TEXT NOT NULL,
    tmdb_id TEXT,
    members TEXT[] NOT NULL DEFAULT '{}',
    PRIMARY KEY (link_id, remote_id)
);
