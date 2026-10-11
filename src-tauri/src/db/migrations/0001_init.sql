-- Initial schema (data-model.md). Constraints quoted from the data model are enforced here
-- where SQLite can express them, and in code otherwise.

CREATE TABLE track (
    id            TEXT PRIMARY KEY,
    title         TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 500),
    artists       TEXT NOT NULL CHECK (json_valid(artists) AND json_type(artists) = 'array'
                                       AND json_array_length(artists) >= 1),
    album         TEXT,
    album_artist  TEXT,
    track_number  INTEGER CHECK (track_number IS NULL OR track_number >= 1),
    year          INTEGER CHECK (year IS NULL OR year BETWEEN 1000 AND 2100),
    duration_ms   INTEGER CHECK (duration_ms IS NULL OR duration_ms > 0),
    isrc          TEXT,
    artwork_hash  TEXT,
    file_path     TEXT UNIQUE,
    file_format   TEXT CHECK (file_format IS NULL OR file_format IN ('m4a', 'opus', 'mp3', 'flac', 'ogg', 'wav')),
    file_mtime    INTEGER,
    file_size     INTEGER,
    origin        TEXT NOT NULL CHECK (origin IN ('downloaded', 'scanned')),
    status        TEXT NOT NULL CHECK (status IN ('pending', 'available', 'missing', 'failed')),
    dedup_key     TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);
CREATE INDEX track_dedup_key ON track (dedup_key);

CREATE VIRTUAL TABLE track_fts USING fts5 (
    track_id UNINDEXED, title, artists, album,
    tokenize = 'unicode61 remove_diacritics 2',
    prefix = '2 3'
);
CREATE TRIGGER track_fts_ai AFTER INSERT ON track BEGIN
    INSERT INTO track_fts (track_id, title, artists, album)
    VALUES (new.id, new.title, (SELECT group_concat(value, ' ') FROM json_each(new.artists)), new.album);
END;
CREATE TRIGGER track_fts_au AFTER UPDATE OF title, artists, album ON track BEGIN
    DELETE FROM track_fts WHERE track_id = old.id;
    INSERT INTO track_fts (track_id, title, artists, album)
    VALUES (new.id, new.title, (SELECT group_concat(value, ' ') FROM json_each(new.artists)), new.album);
END;
CREATE TRIGGER track_fts_ad AFTER DELETE ON track BEGIN
    DELETE FROM track_fts WHERE track_id = old.id;
END;

CREATE TABLE source_ref (
    id                 TEXT PRIMARY KEY,
    track_id           TEXT NOT NULL REFERENCES track (id) ON DELETE CASCADE,
    platform           TEXT NOT NULL,
    platform_track_id  TEXT NOT NULL,
    url                TEXT,
    raw_metadata       TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(raw_metadata)),
    UNIQUE (platform, platform_track_id)
);
CREATE INDEX source_ref_track ON source_ref (track_id);

-- "match" is an SQL keyword, hence track_match.
CREATE TABLE track_match (
    track_id          TEXT PRIMARY KEY REFERENCES track (id) ON DELETE CASCADE,
    youtube_video_id  TEXT NOT NULL CHECK (length(youtube_video_id) = 11),
    confidence        REAL NOT NULL CHECK (confidence BETWEEN 0 AND 1),
    confidence_level  TEXT NOT NULL CHECK (confidence_level IN ('high', 'medium', 'low')),
    method            TEXT NOT NULL CHECK (method IN ('direct', 'auto', 'user_candidate', 'user_url')),
    signals           TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(signals)),
    decided_at        TEXT NOT NULL
);

CREATE TABLE match_candidate (
    track_id          TEXT NOT NULL REFERENCES track_match (track_id) ON DELETE CASCADE,
    youtube_video_id  TEXT NOT NULL,
    title             TEXT NOT NULL,
    channel           TEXT NOT NULL,
    duration_ms       INTEGER,
    score             REAL NOT NULL,
    rank              INTEGER NOT NULL CHECK (rank >= 1),
    PRIMARY KEY (track_id, youtube_video_id)
);

CREATE TABLE playlist (
    id                    TEXT PRIMARY KEY,
    kind                  TEXT NOT NULL CHECK (kind IN ('imported', 'local')),
    name                  TEXT NOT NULL CHECK (length(name) >= 1),
    platform              TEXT,
    platform_playlist_id  TEXT,
    url                   TEXT,
    owner_name            TEXT,
    artwork_hash          TEXT,
    last_synced_at        TEXT,
    fetch_mode            TEXT CHECK (fetch_mode IS NULL OR fetch_mode IN ('official', 'public_fallback')),
    created_at            TEXT NOT NULL,
    CHECK (kind = 'local' OR (platform IS NOT NULL AND platform_playlist_id IS NOT NULL)),
    UNIQUE (platform, platform_playlist_id)
);

CREATE TABLE playlist_entry (
    playlist_id        TEXT NOT NULL REFERENCES playlist (id) ON DELETE CASCADE,
    track_id           TEXT NOT NULL REFERENCES track (id) ON DELETE CASCADE,
    position           INTEGER NOT NULL CHECK (position >= 0),
    removed_online_at  TEXT,
    UNIQUE (playlist_id, position)
);
CREATE INDEX playlist_entry_track ON playlist_entry (track_id);

CREATE TABLE import_batch (
    id           TEXT PRIMARY KEY,
    playlist_id  TEXT REFERENCES playlist (id) ON DELETE SET NULL,
    source_url   TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    finished_at  TEXT
);

CREATE TABLE download_job (
    id             TEXT PRIMARY KEY,
    batch_id       TEXT NOT NULL REFERENCES import_batch (id) ON DELETE CASCADE,
    track_id       TEXT NOT NULL REFERENCES track (id) ON DELETE CASCADE,
    state          TEXT NOT NULL CHECK (state IN ('queued', 'matching', 'awaiting_review', 'downloading',
                                                  'tagging', 'finalizing', 'paused', 'done', 'skipped',
                                                  'failed', 'cancelled')),
    progress       REAL NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 1),
    -- "≤ 5 automatic retries"
    attempts       INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 5),
    error_kind     TEXT CHECK (error_kind IS NULL OR error_kind IN ('not_found', 'unavailable', 'age_restricted',
                                                                    'region_blocked', 'rate_limited', 'network',
                                                                    'disk_full', 'tool_missing', 'unknown')),
    error_message  TEXT,
    temp_path      TEXT,
    updated_at     TEXT NOT NULL
);
CREATE INDEX download_job_batch ON download_job (batch_id);
-- A track is never downloaded twice at once.
CREATE UNIQUE INDEX download_job_active_track ON download_job (track_id)
    WHERE state NOT IN ('done', 'skipped', 'failed', 'cancelled');

CREATE TABLE connected_account (
    platform      TEXT PRIMARY KEY,
    display_name  TEXT NOT NULL,
    client_id     TEXT NOT NULL,
    keyring_key   TEXT NOT NULL,
    scopes        TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(scopes)),
    status        TEXT NOT NULL CHECK (status IN ('connected', 'expired', 'error')),
    connected_at  TEXT NOT NULL
);

CREATE TABLE watched_folder (
    id                  TEXT PRIMARY KEY,
    path                TEXT NOT NULL UNIQUE,
    is_download_target  INTEGER NOT NULL DEFAULT 0 CHECK (is_download_target IN (0, 1)),
    last_scan_at        TEXT
);
-- Exactly one folder is the download target ("music folder").
CREATE UNIQUE INDEX watched_folder_one_target ON watched_folder (is_download_target)
    WHERE is_download_target = 1;

CREATE TABLE organize_plan (
    id            TEXT PRIMARY KEY,
    pattern       TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    state         TEXT NOT NULL CHECK (state IN ('previewed', 'applied', 'cancelled', 'partially_applied')),
    journal_path  TEXT
);

CREATE TABLE organize_op (
    plan_id    TEXT NOT NULL REFERENCES organize_plan (id) ON DELETE CASCADE,
    track_id   TEXT NOT NULL REFERENCES track (id) ON DELETE CASCADE,
    op         TEXT NOT NULL CHECK (op IN ('move', 'retag')),
    from_path  TEXT NOT NULL,
    to_path    TEXT NOT NULL,
    conflict   TEXT NOT NULL DEFAULT 'none' CHECK (conflict IN ('none', 'exists', 'invalid_name', 'too_long'))
);

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL CHECK (json_valid(value))
);

-- Artists and albums are derived from tracks (data-model.md), not stored.
CREATE VIEW artist_view AS
    SELECT name, count(DISTINCT track_id) AS track_count
    FROM (
        SELECT t.id AS track_id, j.value AS name FROM track t, json_each(t.artists) j
        UNION
        SELECT t.id, t.album_artist FROM track t WHERE t.album_artist IS NOT NULL
    )
    GROUP BY name COLLATE NOCASE;

CREATE VIEW album_view AS
    SELECT
        lower(coalesce(album_artist, json_extract(artists, '$[0]'))) || '|' || lower(album) AS album_key,
        coalesce(album_artist, json_extract(artists, '$[0]')) AS album_artist,
        album,
        count(*) AS track_count,
        min(year) AS year,
        (SELECT t2.artwork_hash FROM track t2
          WHERE t2.album = t.album AND t2.artwork_hash IS NOT NULL
            AND coalesce(t2.album_artist, json_extract(t2.artists, '$[0]'))
              = coalesce(t.album_artist, json_extract(t.artists, '$[0]'))
          GROUP BY t2.artwork_hash ORDER BY count(*) DESC LIMIT 1) AS artwork_hash
    FROM track t
    WHERE album IS NOT NULL
    GROUP BY album_key;
