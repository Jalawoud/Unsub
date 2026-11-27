use tempfile::TempDir;
use unsub_library::{Library, Track, TrackQuery};

use crate::common::{copy_fixtures, fixtures_dir, library_of, scan, track};

fn row(id: i64, title: &str, artist: &str, album: &str, duration_ms: u64) -> Track {
    track(id, title, artist, album, duration_ms)
}

fn catalogue() -> Library {
    library_of(&[
        row(1, "Song", "Artist", "Album", 200_000),
        row(
            2,
            "Song (Remastered 2009)",
            "Artist feat. Guest",
            "Album",
            201_500,
        ),
        row(3, "song", "ARTIST", "Other Album", 200_500),
        row(4, "Song (Live)", "Artist", "Album", 200_000),
        row(5, "Song", "Other Artist", "Album", 200_000),
        row(6, "Song", "Artist", "Album", 215_000),
        row(7, "Another", "Band", "Record", 180_000),
        row(8, "Another", "Band", "Record", 180_000),
        row(9, "Unique", "Solo", "Thing", 1_000),
        // Nothing to compare: no artist, no title.
        row(10, "Orphan", "", "", 1_000),
        row(11, "Orphan", "", "", 1_000),
        row(12, "", "Ghost", "", 1_000),
        row(13, "", "Ghost", "", 1_000),
    ])
}

#[test]
fn duplicates_match_on_normalized_title_and_artist() {
    let library = catalogue();

    let groups = library.duplicates(false).unwrap();

    assert_eq!(groups, [vec![1, 2, 3, 6], vec![7, 8]]);
}

#[test]
fn exact_duplicates_also_need_the_same_album_and_a_length_within_two_seconds() {
    let library = catalogue();

    let groups = library.duplicates(true).unwrap();

    // 3 is on another album and 6 is fifteen seconds longer; both drop out.
    assert_eq!(groups, [vec![1, 2], vec![7, 8]]);
}

#[test]
fn exact_groups_never_span_more_than_the_tolerance() {
    let library = library_of(&[
        row(1, "Chain", "Band", "Album", 100_000),
        row(2, "Chain", "Band", "Album", 101_500),
        row(3, "Chain", "Band", "Album", 103_000),
        row(4, "Chain", "Band", "Album", 103_500),
        row(5, "Chain", "Band", "Album", 150_000),
    ]);

    let groups = library.duplicates(true).unwrap();

    // 100.0 s and 101.5 s are within two seconds of each other; 103.0 s is not
    // within two seconds of 100.0 s, so it starts the next group with 103.5 s.
    assert_eq!(groups, [vec![1, 2], vec![3, 4]]);
}

#[test]
fn two_albums_without_a_name_count_as_the_same_album() {
    let library = library_of(&[
        row(1, "Single", "Band", "", 100_000),
        row(2, "Single", "Band", "", 100_000),
        row(3, "Single", "Band", "Named", 100_000),
    ]);

    assert_eq!(library.duplicates(true).unwrap(), [vec![1, 2]]);
    assert_eq!(library.duplicates(false).unwrap(), [vec![1, 2, 3]]);
}

#[test]
fn groups_come_in_order_of_their_lowest_id_with_ids_ascending() {
    let library = library_of(&[
        row(30, "B", "X", "A", 1_000),
        row(10, "A", "X", "A", 1_000),
        row(20, "B", "X", "A", 1_000),
        row(40, "A", "X", "A", 1_000),
    ]);

    assert_eq!(
        library.duplicates(false).unwrap(),
        [vec![10, 40], vec![20, 30]]
    );
}

#[test]
fn no_duplicates_and_an_empty_library_give_no_groups() {
    assert!(
        Library::open_in_memory()
            .unwrap()
            .duplicates(false)
            .unwrap()
            .is_empty()
    );
    let dir = copy_fixtures();
    let mut library = Library::open_in_memory().unwrap();
    scan(&mut library, dir.path());

    assert!(library.duplicates(false).unwrap().is_empty());
    assert!(library.duplicates(true).unwrap().is_empty());
}

#[test]
fn a_scanned_copy_of_a_file_is_found_in_both_modes() {
    let dir = copy_fixtures();
    std::fs::copy(
        fixtures_dir().join("halo.mp3"),
        dir.path().join("halo copy.mp3"),
    )
    .unwrap();
    let mut library = Library::open_in_memory().unwrap();
    scan(&mut library, dir.path());

    let both: Vec<usize> = [false, true]
        .iter()
        .map(|exact| {
            let groups = library.duplicates(*exact).unwrap();
            assert_eq!(groups.len(), 1);
            groups[0].len()
        })
        .collect();

    assert_eq!(both, [2, 2]);
}

/// A folder holding only the halo.mp3 fixture, under `name`.
fn folder_with_halo(name: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::copy(fixtures_dir().join("halo.mp3"), dir.path().join(name)).unwrap();
    dir
}

/// What the library knows halo.mp3 as, read the way an import reads it.
fn halo() -> Track {
    let dir = folder_with_halo("halo.mp3");
    let mut library = Library::open_in_memory().unwrap();
    scan(&mut library, dir.path());
    library.tracks(&TrackQuery::default()).unwrap().remove(0)
}

fn library_holding_halo(offset_ms: i64, album: Option<&str>) -> Library {
    let halo = halo();
    let duration = (halo.duration_ms as i64 + offset_ms) as u64;
    library_of(&[row(1, &halo.title, &halo.artist, album.unwrap_or(&halo.album), duration)])
}

#[test]
fn a_checked_add_holds_back_a_song_the_library_already_has() {
    let mut library = library_holding_halo(0, None);
    let dir = folder_with_halo("copy of halo.mp3");

    let summary = library.add_paths_checked(&[dir.path().to_path_buf()], |_| {}).unwrap();

    assert_eq!(summary.added, 0);
    assert_eq!(summary.held.len(), 1);
    assert!(summary.held[0].path.ends_with("copy of halo.mp3"));
    assert_eq!(summary.held[0].existing.path, "/music/1.mp3");
    assert_eq!(library.tracks(&TrackQuery::default()).unwrap().len(), 1);
}

#[test]
fn lengths_up_to_fifteen_seconds_apart_are_the_same_song() {
    for (offset, held) in [(0, true), (15_000, true), (15_001, false), (60_000, false)] {
        let mut library = library_holding_halo(offset, None);
        let dir = folder_with_halo("halo.mp3");

        let summary = library.add_paths_checked(&[dir.path().to_path_buf()], |_| {}).unwrap();

        assert_eq!(summary.held.len() == 1, held, "{offset} ms apart");
        assert_eq!(summary.added == 1, !held, "{offset} ms apart");
    }
}

#[test]
fn the_same_song_on_another_album_is_not_held() {
    let mut library = library_holding_halo(0, Some("Greatest Hits"));
    let dir = folder_with_halo("halo.mp3");

    let summary = library.add_paths_checked(&[dir.path().to_path_buf()], |_| {}).unwrap();

    assert!(summary.held.is_empty());
    assert_eq!(summary.added, 1);
}

#[test]
fn two_copies_in_one_import_keep_the_first_and_hold_the_second() {
    let mut library = Library::open_in_memory().unwrap();
    let dir = folder_with_halo("a.mp3");
    std::fs::copy(dir.path().join("a.mp3"), dir.path().join("b.mp3")).unwrap();

    let summary = library.add_paths_checked(&[dir.path().to_path_buf()], |_| {}).unwrap();

    assert_eq!(summary.added, 1);
    assert_eq!(summary.held.len(), 1);
    let kept = &library.tracks(&TrackQuery::default()).unwrap()[0].path;
    assert_eq!(&summary.held[0].existing.path, kept);
    assert_ne!(&summary.held[0].path, kept);
}

#[test]
fn rescanning_songs_already_in_the_library_holds_nothing() {
    let dir = folder_with_halo("halo.mp3");
    let mut library = Library::open_in_memory().unwrap();
    scan(&mut library, dir.path());

    let summary = library.add_paths_checked(&[dir.path().to_path_buf()], |_| {}).unwrap();

    assert!(summary.held.is_empty());
    assert_eq!(summary.unchanged, 1);
}

#[test]
fn a_plain_add_still_adds_duplicates() {
    let mut library = library_holding_halo(0, None);
    let dir = folder_with_halo("halo.mp3");

    let summary = library.add_paths(&[dir.path().to_path_buf()], |_| {}).unwrap();

    assert!(summary.held.is_empty());
    assert_eq!(summary.added, 1);
}
