use super::{make_temp_dir, with_temp_dir_override};
use std::{fs, path::Path};

#[test]
fn make_temp_dir_ignores_cache_root_override() {
let temp_dir = make_temp_dir("pipeline-temp-dir-recover");
let bad_tmp = temp_dir.join("not-a-directory");
fs::write(&bad_tmp, "cache root blocker").expect("bad tmp file should exist");
let recovered =
    with_temp_dir_override(&bad_tmp, || make_temp_dir("pipeline-temp-dir-recovered"));

assert!(recovered.is_dir());
assert!(!recovered.starts_with(&bad_tmp));
if cfg!(unix) {
    assert_eq!(recovered.parent(), Some(Path::new("/tmp")));
}

fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
fs::remove_dir_all(recovered).expect("recovered temp dir should be removed");
}
