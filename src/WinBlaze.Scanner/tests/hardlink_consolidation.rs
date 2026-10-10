use std::fs;
use winblaze_scanner::hardlinks::consolidate;
#[test]
fn verified_duplicates_become_aliases_without_changing_their_bytes() {
    let root = std::env::temp_dir().join(format!("wb-consolidate-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let master = root.join("master");
    let duplicate = root.join("duplicate");
    fs::write(&master, b"verified identical contents").unwrap();
    fs::write(&duplicate, b"verified identical contents").unwrap();
    consolidate(&master, &duplicate).unwrap();
    assert_eq!(
        fs::read(&duplicate).unwrap(),
        b"verified identical contents"
    );
    let first = winblaze_scanner::file_details::query_file_details(&master).unwrap();
    let second = winblaze_scanner::file_details::query_file_details(&duplicate).unwrap();
    assert_eq!(first.identity, second.identity);
    assert_eq!(first.links, 2);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
    fs::remove_file(master).unwrap();
    fs::remove_file(duplicate).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn rejects_different_bytes_and_named_streams_without_touching_either_file() {
    let root = std::env::temp_dir().join(format!("wb-consolidate-refuse-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let master = root.join("master");
    let duplicate = root.join("duplicate");
    fs::write(&master, b"abc").unwrap();
    fs::write(&duplicate, b"abd").unwrap();
    assert!(consolidate(&master, &duplicate)
        .unwrap_err()
        .to_string()
        .contains("differ"));
    assert_eq!(fs::read(&master).unwrap(), b"abc");
    assert_eq!(fs::read(&duplicate).unwrap(), b"abd");
    fs::write(&duplicate, b"abc").unwrap();
    fs::write(
        duplicate.with_file_name("duplicate:note"),
        b"important metadata",
    )
    .unwrap();
    assert!(consolidate(&master, &duplicate)
        .unwrap_err()
        .to_string()
        .contains("streams"));
    assert_eq!(
        fs::read(duplicate.with_file_name("duplicate:note")).unwrap(),
        b"important metadata"
    );
    assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
    fs::remove_file(master).unwrap();
    fs::remove_file(duplicate).unwrap();
    fs::remove_dir(root).unwrap();
}
