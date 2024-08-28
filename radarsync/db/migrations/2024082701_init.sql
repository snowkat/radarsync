CREATE TABLE devices (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE files (
    id INTEGER PRIMARY KEY NOT NULL,
    path TEXT UNIQUE NOT NULL,
    meta_id INTEGER NOT NULL
);

CREATE TABLE metadata (
    id INTEGER PRIMARY KEY NOT NULL,
    device_id INTEGER NOT NULL,
    title TEXT,
    artist TEXT,
    album TEXT,
    trackno TEXT,
    mbid TEXT
);