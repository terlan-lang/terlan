use super::find_worker;

#[test]
fn discovery_retains_not_found_kind_and_bounds_ancestor_search() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("one/two/three");
    std::fs::create_dir_all(&bin).unwrap();
    let current = bin.join("terlc");
    let name = if cfg!(windows) {
        "terlan-native-worker.exe"
    } else {
        "terlan-native-worker"
    };
    let outside = root.path().join(name);
    std::fs::write(&outside, []).unwrap();
    let error = find_worker(&current).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(error.to_string().contains("capability_worker_missing"));

    let within = root.path().join("one").join(name);
    std::fs::create_dir(&within).unwrap();
    assert_eq!(
        find_worker(&current).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    std::fs::remove_dir(&within).unwrap();
    std::fs::write(&within, []).unwrap();
    assert_eq!(
        find_worker(&current).unwrap(),
        within.canonicalize().unwrap()
    );

    let adjacent = bin.join(name);
    std::fs::write(&adjacent, []).unwrap();
    assert_eq!(
        find_worker(&current).unwrap(),
        adjacent.canonicalize().unwrap()
    );
}
