use super::*;

#[test]
fn release_does_not_wait_for_duplicated_file_description_to_close() {
    let path = std::env::temp_dir().join(format!(
        "terlan-cache-lease-description-{}",
        std::process::id()
    ));
    let file = File::create_new(&path).unwrap();
    drop(file);
    let owner = lease(&path).unwrap().unwrap();
    let inherited = owner.0.try_clone().unwrap();
    assert!(lease(&path).unwrap().is_none());
    drop(owner);
    let next = lease(&path).unwrap().expect("explicitly released lease");
    drop(inherited);
    assert!(lease(&path).unwrap().is_none());
    drop(next);
    fs::remove_file(path).unwrap();
}
