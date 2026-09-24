use assert_cmd::cargo::cargo_bin_cmd;
use std::fs::{self, File, FileTimes};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

fn complete_apply(source: &Path, paths: &Path, home: &Path) {
    cargo_bin_cmd!()
        .arg("complete-apply")
        .args([source, paths, home])
        .assert()
        .success();
}

fn identity(path: &Path) -> (u64, i64, i64, u32) {
    let metadata = fs::metadata(path).unwrap();
    (
        metadata.ino(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.mode(),
    )
}

#[test]
fn unchanged_files_and_subtrees_keep_their_inodes_and_mtimes() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let home = temp.path().join("home");
    fs::create_dir_all(source.join("home/managed/nested")).unwrap();
    for path in [
        ".gitconfig",
        "managed/changed",
        "managed/nested/unchanged",
        "managed/unchanged",
    ] {
        fs::write(source.join("home").join(path), "original").unwrap();
        fs::set_permissions(
            source.join("home").join(path),
            fs::Permissions::from_mode(0o444),
        )
        .unwrap();
    }
    let paths = temp.path().join("paths.json");
    fs::write(&paths, "[\".gitconfig\", \"managed\"]").unwrap();
    complete_apply(&source, &paths, &home);
    let preserved = [
        ".gitconfig",
        "managed/nested",
        "managed/nested/unchanged",
        "managed/unchanged",
    ];
    for path in preserved {
        File::open(home.join(path))
            .unwrap()
            .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(123456)))
            .unwrap();
    }
    let before: Vec<_> = preserved
        .iter()
        .map(|path| identity(&home.join(path)))
        .collect();
    complete_apply(&source, &paths, &home);
    assert_eq!(
        before,
        preserved
            .iter()
            .map(|path| identity(&home.join(path)))
            .collect::<Vec<_>>()
    );
    fs::set_permissions(
        source.join("home/managed/changed"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    fs::write(source.join("home/managed/changed"), "new").unwrap();
    fs::write(home.join("managed/obsolete"), "remove").unwrap();
    complete_apply(&source, &paths, &home);
    assert_eq!(
        before,
        preserved
            .iter()
            .map(|path| identity(&home.join(path)))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        fs::read_to_string(home.join("managed/changed")).unwrap(),
        "new"
    );
    assert!(!home.join("managed/obsolete").exists());
    fs::set_permissions(
        source.join("home/managed/changed"),
        fs::Permissions::from_mode(0o555),
    )
    .unwrap();
    complete_apply(&source, &paths, &home);
    assert_eq!(
        fs::metadata(home.join("managed/changed")).unwrap().mode() & 0o777,
        0o755
    );
    assert_eq!(
        before,
        preserved
            .iter()
            .map(|path| identity(&home.join(path)))
            .collect::<Vec<_>>()
    );
}

#[test]
fn readers_observe_complete_old_or_new_files_during_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let home = temp.path().join("home");
    fs::create_dir_all(source.join("home")).unwrap();
    let old = vec![b'a'; 4 * 1024 * 1024];
    let new = vec![b'b'; 4 * 1024 * 1024];
    fs::write(source.join("home/managed"), &old).unwrap();
    fs::set_permissions(
        source.join("home/managed"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let paths = temp.path().join("paths.json");
    fs::write(&paths, "[\"managed\"]").unwrap();
    complete_apply(&source, &paths, &home);
    let mut opened_before = File::open(home.join("managed")).unwrap();
    let running = Arc::new(AtomicBool::new(true));
    let (ready, received) = mpsc::sync_channel(0);
    let reader = {
        let running = Arc::clone(&running);
        let target = home.join("managed");
        let old = old.clone();
        let new = new.clone();
        thread::spawn(move || {
            assert_eq!(fs::read(&target).unwrap(), old);
            ready.send(()).unwrap();
            let mut samples = 0;
            while running.load(Ordering::Relaxed) {
                let content =
                    fs::read(&target).expect("managed file disappeared while being replaced");
                assert!(
                    content == old || content == new,
                    "reader observed a partial file"
                );
                assert_eq!(fs::metadata(&target).unwrap().mode() & 0o777, 0o644);
                samples += 1;
            }
            samples
        })
    };
    received.recv().unwrap();
    for content in [&new, &old, &new, &old, &new] {
        fs::write(source.join("home/managed"), content).unwrap();
        complete_apply(&source, &paths, &home);
    }
    running.store(false, Ordering::Relaxed);
    assert!(reader.join().unwrap() > 0);
    let mut previous = Vec::new();
    opened_before.read_to_end(&mut previous).unwrap();
    assert_eq!(previous, old);
    assert_eq!(fs::read(home.join("managed")).unwrap(), new);
    assert_eq!(fs::read_dir(&home).unwrap().count(), 1);
}
