use git_autosnap::core::runtime::process::{acquire_lock, pid_file};
use tempfile::tempdir;

#[test]
fn pidfile_lock_is_exclusive() {
    let td = tempdir().unwrap();
    let root = td.path();

    // First lock succeeds
    let guard = acquire_lock(root).expect("first lock");
    assert!(pid_file(root).exists());

    // Second lock fails while first is held
    match acquire_lock(root) {
        Ok(_) => panic!("second lock should fail"),
        Err(e) => {
            let msg = format!("{e}");
            assert!(msg.contains("already running"));
        }
    }

    // Drop first; subsequent lock should succeed again
    drop(guard);
    let _guard2 = acquire_lock(root).expect("relock after drop");
}
