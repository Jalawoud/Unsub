use std::collections::{BTreeMap, HashMap};

use crate::error::Result;
use crate::library::Library;
use crate::model::{DuplicateOf, RADIO, Track};
use crate::normalize::{artist_key, title_key};

/// Within this many milliseconds of each other, two lengths are the same recording.
const SAME_LENGTH_MS: u64 = 2_000;

struct Row {
    id: i64,
    album: Option<String>,
    duration_ms: u64,
}

impl Library {
    /// Groups of tracks that look like the same song twice. Tracks match on
    /// title and primary artist as the matcher reads them (so a remaster
    /// still matches, a live version does not). `exact` is iTunes' "Show Exact
    /// Duplicates": the album must match too, and lengths within two seconds.
    /// Groups are ordered by their lowest id, and ids within one ascend.
    pub fn duplicates(&self, exact: bool) -> Result<Vec<Vec<i64>>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, title, artist, album, duration_ms FROM tracks
                 WHERE kind <> {RADIO} ORDER BY id"
        ))?;
        let mut rows = stmt.query([])?;
        let mut buckets: BTreeMap<(String, String), Vec<Row>> = BTreeMap::new();
        while let Some(row) = rows.next()? {
            let title: String = row.get(1)?;
            let artist: String = row.get(2)?;
            // Without a title and an artist there is nothing to compare.
            if let (Some(title), Some(artist)) = (title_key(&title), artist_key(&artist)) {
                let album: String = row.get(3)?;
                buckets.entry((title, artist)).or_default().push(Row {
                    id: row.get(0)?,
                    album: title_key(&album),
                    duration_ms: row.get(4)?,
                });
            }
        }

        let mut groups: Vec<Vec<i64>> = buckets
            .into_values()
            .flat_map(|bucket| {
                if exact {
                    exact_groups(bucket)
                } else {
                    vec![bucket]
                }
            })
            .filter(|group| group.len() > 1)
            .map(|group| group.iter().map(|row| row.id).collect())
            .collect();
        groups.sort_by_key(|group| group[0]);
        Ok(groups)
    }
}

/// Splits a bucket by album, then by length: each group starts at its shortest
/// track and takes those within the tolerance of it, so no two members are
/// further apart than that.
fn exact_groups(bucket: Vec<Row>) -> Vec<Vec<Row>> {
    let mut by_album: BTreeMap<Option<String>, Vec<Row>> = BTreeMap::new();
    for row in bucket {
        by_album.entry(row.album.clone()).or_default().push(row);
    }
    let mut groups = Vec::new();
    for mut rows in by_album.into_values() {
        rows.sort_by_key(|row| (row.duration_ms, row.id));
        let mut current: Vec<Row> = Vec::new();
        for row in rows {
            if current
                .first()
                .is_some_and(|first| row.duration_ms - first.duration_ms > SAME_LENGTH_MS)
            {
                groups.push(std::mem::take(&mut current));
            }
            current.push(row);
        }
        groups.push(current);
    }
    for group in &mut groups {
        group.sort_by_key(|row| row.id);
    }
    groups
}

/// When adding music, two lengths within this many milliseconds are the same song. Looser than
/// "Show Exact Duplicates" because a rip, a download and a store copy of one song often differ by
/// a few seconds of silence or a longer fade.
pub const ADD_DUPLICATE_TOLERANCE_MS: u64 = 15_000;

type SongKey = (String, String);

/// The library's songs by title and primary artist, for an import to check new files
/// against. Songs the import adds are put in too, so one folder holding two copies is caught.
pub(crate) struct DuplicateIndex {
    songs: HashMap<SongKey, Vec<DuplicateOf>>,
}

impl DuplicateIndex {
    pub(crate) fn of(library: &Library) -> Result<Self> {
        let mut stmt = library.conn.prepare(&format!(
            "SELECT title, artist, path, duration_ms, codec, bitrate_kbps FROM tracks
                 WHERE kind <> {RADIO}"
        ))?;
        let mut rows = stmt.query([])?;
        let mut index = Self { songs: HashMap::new() };
        while let Some(row) = rows.next()? {
            let (title, artist): (String, String) = (row.get(0)?, row.get(1)?);
            if let Some(key) = song_key(&title, &artist) {
                index.songs.entry(key).or_default().push(DuplicateOf {
                    path: row.get(2)?,
                    duration_ms: row.get(3)?,
                    codec: row.get(4)?,
                    bitrate_kbps: row.get(5)?,
                });
            }
        }
        Ok(index)
    }

    /// The song already known that `track` repeats, if any; the closest in length when several do.
    pub(crate) fn find(&self, track: &Track) -> Option<&DuplicateOf> {
        let key = song_key(&track.title, &track.artist)?;
        self.songs
            .get(&key)?
            .iter()
            .filter(|song| song.path != track.path)
            .map(|song| (song.duration_ms.abs_diff(track.duration_ms), song))
            .filter(|(gap, _)| *gap <= ADD_DUPLICATE_TOLERANCE_MS)
            .min_by_key(|(gap, _)| *gap)
            .map(|(_, song)| song)
    }

    pub(crate) fn insert(&mut self, track: &Track) {
        if let Some(key) = song_key(&track.title, &track.artist) {
            self.songs.entry(key).or_default().push(DuplicateOf {
                path: track.path.clone(),
                duration_ms: track.duration_ms,
                codec: track.codec.clone(),
                bitrate_kbps: track.bitrate_kbps,
            });
        }
    }
}

/// Keys as "Show Duplicates" does, ignoring the album: a single, a greatest hits and the original
/// album often hold the same recording. Without a title and an artist there is nothing to compare.
fn song_key(title: &str, artist: &str) -> Option<SongKey> {
    Some((title_key(title)?, artist_key(artist)?))
}
