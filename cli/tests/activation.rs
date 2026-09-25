use assert_cmd::cargo::cargo_bin;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

struct Fixture {
    root: tempfile::TempDir,
    children: Vec<Child>,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Self {
            root: tempfile::tempdir().unwrap(),
            children: Vec::new(),
        };
        fixture.executable(
            "lix/bin/nix-env",
            "#!/bin/sh\nprintf 'profile\\n' >> \"$FIXTURE_ROOT/order\"\n",
        );
        fixture.executable(
            "system/sw/bin/darwin-rebuild",
            r#"#!/bin/sh
printf '%s\n' "$PPID" > "$FIXTURE_ROOT/owner"
if ! mkdir "$FIXTURE_ROOT/active" 2>/dev/null; then
  touch "$FIXTURE_ROOT/overlap"
  exit 1
fi
trap 'rmdir "$FIXTURE_ROOT/active"' EXIT
printf 'activation\n' >> "$FIXTURE_ROOT/order"
printf 'entered\n' >> "$FIXTURE_ROOT/entries"
if [ -e "$FIXTURE_ROOT/fail" ]; then exit 42; fi
while [ ! -e "$FIXTURE_ROOT/release" ]; do sleep 0.01; done
"#,
        );
        fs::create_dir(fixture.path("etc")).unwrap();
        fs::create_dir_all(fixture.path("source/home")).unwrap();
        fs::create_dir(fixture.path("home")).unwrap();
        fs::write(fixture.path("paths.json"), "[]").unwrap();
        fixture
    }

    fn path(&self, path: &str) -> PathBuf {
        self.root.path().join(path)
    }

    fn executable(&self, path: &str, content: &str) {
        let path = self.path(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn start(&mut self, expected: &str, desired: &str) -> usize {
        self.start_with(expected, desired, &[])
    }

    fn start_with(&mut self, expected: &str, desired: &str, options: &[&str]) -> usize {
        let user = Command::new("/usr/bin/id").arg("-un").output().unwrap();
        assert!(user.status.success());
        let index = self.children.len();
        let child = Command::new(cargo_bin!("dotfiles"))
            .arg("apply-built")
            .args(options)
            .arg(self.path("lix/bin/nix"))
            .arg(String::from_utf8(user.stdout).unwrap().trim())
            .arg(self.path("system").canonicalize().unwrap())
            .arg(self.path("etc/flake.nix"))
            .args([expected, desired])
            .args([
                self.path("source"),
                self.path("paths.json"),
                self.path("home"),
            ])
            .env("FIXTURE_ROOT", self.root.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(File::create(self.path(&format!("stderr-{index}"))).unwrap())
            .process_group(0)
            .spawn()
            .unwrap();
        self.children.push(child);
        index
    }

    fn errors(&self, index: usize) -> String {
        fs::read_to_string(self.path(&format!("stderr-{index}"))).unwrap()
    }

    fn apply_via_backend(&self, options: &[&str]) -> Output {
        let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../lib/install-system.sh");
        let user = Command::new("/usr/bin/id").arg("-un").output().unwrap();
        assert!(user.status.success());
        Command::new("/bin/sh")
            .args([
                "-eu",
                "-c",
                ". \"$1\"; shift; install_system_apply_built_system \"$@\"",
                "activation-backend-test",
            ])
            .arg(library)
            .arg(self.path("sudo"))
            .arg("/usr/bin/env")
            .arg(self.path("lix/bin/nix"))
            .arg(String::from_utf8(user.stdout).unwrap().trim())
            .arg(self.path("system").canonicalize().unwrap())
            .arg(self.path("etc/flake.nix"))
            .args(["missing", "/source/flake.nix"])
            .args([
                self.path("legacy-dotfiles"),
                self.path("source"),
                self.path("paths.json"),
                self.path("home"),
            ])
            .args(options)
            .env("FIXTURE_ROOT", self.root.path())
            .output()
            .unwrap()
    }

    fn finish(&mut self, index: usize) -> ExitStatus {
        let mut status = None;
        wait_for(|| {
            status = self.children[index].try_wait().unwrap();
            status.is_some()
        });
        status.unwrap()
    }

    fn run(&mut self, expected: &str, desired: &str, code: i32) {
        let index = self.start(expected, desired);
        assert_eq!(
            self.finish(index).code(),
            Some(code),
            "{}",
            self.errors(index)
        );
    }

    fn release(&self) {
        fs::write(self.path("release"), "").unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for child in &mut self.children {
            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            let _ = child.wait();
        }
    }
}

fn wait_for(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "process did not reach the expected state"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "requires the repository shell backend"]
fn backend_upgrades_an_incompatible_caller_and_preserves_copy_only_behavior() {
    for copy_only in [false, true] {
        let fixture = Fixture::new();
        fixture.executable("sudo", "#!/bin/sh\nexec \"$@\"\n");
        fixture.executable(
            "legacy-dotfiles",
            "#!/bin/sh\nprintf \"error: unrecognized subcommand 'apply-built'\\n\" >&2\nexit 2\n",
        );
        symlink(
            cargo_bin!("dotfiles"),
            fixture.path("system/sw/bin/dotfiles"),
        )
        .unwrap();
        fixture.release();
        fs::write(fixture.path("paths.json"), "[\"managed\"]").unwrap();
        fs::write(fixture.path("source/home/managed"), "frozen").unwrap();
        let generation = fixture.path("system").canonicalize().unwrap();
        let current = fixture.path("current-system");
        symlink(&generation, &current).unwrap();
        let options = if copy_only {
            fs::remove_file(fixture.path("lix/bin/nix-env")).unwrap();
            fs::remove_file(fixture.path("system/sw/bin/darwin-rebuild")).unwrap();
            vec![
                "--copy-only",
                "--current-generation",
                current.to_str().unwrap(),
            ]
        } else {
            Vec::new()
        };
        let output = fixture.apply_via_backend(&options);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(fixture.path("home/managed")).unwrap(),
            "frozen"
        );
        assert_eq!(
            fs::read_link(fixture.path("etc/flake.nix")).unwrap(),
            Path::new("/source/flake.nix")
        );
        if copy_only {
            assert!(!fixture.path("order").exists());
            assert_eq!(current.canonicalize().unwrap(), generation);
        } else {
            assert_eq!(
                fs::read_to_string(fixture.path("order")).unwrap(),
                "profile\nactivation\n"
            );
        }
    }
}

#[test]
#[ignore = "requires the repository shell backend"]
fn backend_rejects_an_unavailable_generation_cli_before_privileged_changes() {
    for missing in [false, true] {
        let fixture = Fixture::new();
        fixture.executable(
            "sudo",
            "#!/bin/sh\ntouch \"$FIXTURE_ROOT/privileged\"\nexit 99\n",
        );
        if !missing {
            fs::write(fixture.path("system/sw/bin/dotfiles"), "not executable").unwrap();
        }
        let output = fixture.apply_via_backend(&[]);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("built system has no dotfiles CLI")
        );
        assert!(!fixture.path("privileged").exists());
        assert!(!fixture.path("order").exists());
        assert!(!fixture.path("etc/flake.nix").is_symlink());
    }
}

#[test]
fn concurrent_activations_share_one_stable_lock_even_with_stale_contents() {
    let mut fixture = Fixture::new();
    let lock = fixture.path("etc/flake.nix.apply.lock");
    fs::write(&lock, "2147483647\n").unwrap();
    let inode = fs::metadata(&lock).unwrap().ino();
    let contenders: Vec<_> = (0..4)
        .map(|_| fixture.start("missing", "/source/flake.nix"))
        .collect();
    wait_for(|| fixture.path("entries").exists());
    wait_for(|| {
        fixture
            .children
            .iter_mut()
            .filter_map(|child| child.try_wait().unwrap())
            .count()
            == 3
    });
    assert!(!fixture.path("overlap").exists());
    assert_eq!(
        fs::read_to_string(fixture.path("entries")).unwrap(),
        "entered\n"
    );
    let owner = contenders
        .iter()
        .copied()
        .find(|&index| fixture.children[index].try_wait().unwrap().is_none())
        .unwrap();
    for index in contenders.into_iter().filter(|&index| index != owner) {
        assert!(!fixture.finish(index).success());
        assert!(fixture
            .errors(index)
            .contains("system apply is already running"));
    }
    assert!(!fixture.path("etc/flake.nix").is_symlink());
    fixture.release();
    assert!(fixture.finish(owner).success(), "{}", fixture.errors(owner));
    assert_eq!(
        fs::read_link(fixture.path("etc/flake.nix")).unwrap(),
        Path::new("/source/flake.nix")
    );
    fixture.run("/source/flake.nix", "/next/flake.nix", 0);
    assert_eq!(fs::metadata(lock).unwrap().ino(), inode);
}

#[test]
fn activation_failure_releases_the_lock_and_preserves_the_record() {
    let mut fixture = Fixture::new();
    symlink("/previous/flake.nix", fixture.path("etc/flake.nix")).unwrap();
    fs::write(fixture.path("fail"), "").unwrap();
    fixture.run("/previous/flake.nix", "/source/flake.nix", 42);
    assert_eq!(
        fs::read_link(fixture.path("etc/flake.nix")).unwrap(),
        Path::new("/previous/flake.nix")
    );
    fs::remove_file(fixture.path("fail")).unwrap();
    fixture.release();
    fixture.run("/previous/flake.nix", "/source/flake.nix", 0);
}

#[test]
fn terminated_owners_leave_live_activation_descendants_locked() {
    for signal in [libc::SIGHUP, libc::SIGINT, libc::SIGTERM, libc::SIGKILL] {
        let mut fixture = Fixture::new();
        let owner = fixture.start("missing", "/source/flake.nix");
        wait_for(|| fixture.path("entries").exists());
        assert_eq!(
            unsafe { libc::kill(fixture.children[owner].id() as i32, signal) },
            0
        );
        assert!(!fixture.finish(owner).success());
        let rejected = fixture.start("missing", "/source/flake.nix");
        assert!(!fixture.finish(rejected).success());
        assert!(fixture
            .errors(rejected)
            .contains("system apply is already running"));
        assert!(!fixture.path("overlap").exists());
        fixture.release();
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(fixture.path("etc/flake.nix.apply.lock"))
            .unwrap();
        wait_for(|| fs2::FileExt::try_lock_exclusive(&lock).is_ok());
        drop(lock);
        assert!(!fixture.path("etc/flake.nix").is_symlink());
        fixture.run("missing", "/source/flake.nix", 0);
    }
}

#[test]
fn symlink_and_nonregular_locks_are_rejected_without_activation() {
    for kind in ["symlink", "directory", "fifo"] {
        let mut fixture = Fixture::new();
        let lock = fixture.path("etc/flake.nix.apply.lock");
        fs::write(fixture.path("outside"), "keep").unwrap();
        match kind {
            "symlink" => symlink(fixture.path("outside"), &lock).unwrap(),
            "directory" => fs::create_dir(&lock).unwrap(),
            "fifo" => {
                use std::os::unix::ffi::OsStrExt;
                let path = std::ffi::CString::new(lock.as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            _ => unreachable!(),
        }
        fixture.run("missing", "/source/flake.nix", 1);
        assert_eq!(fs::read_to_string(fixture.path("outside")).unwrap(), "keep");
        assert!(!fixture.path("order").exists());
    }
}

#[test]
fn source_record_is_checked_before_activation_and_again_before_commit() {
    let mut fixture = Fixture::new();
    fixture.run("/previous/flake.nix", "/source/flake.nix", 1);
    assert!(!fixture.path("order").exists());
    let owner = fixture.start("missing", "/source/flake.nix");
    wait_for(|| fixture.path("entries").exists());
    symlink("/other/flake.nix", fixture.path("etc/flake.nix")).unwrap();
    fixture.release();
    assert!(!fixture.finish(owner).success());
    assert!(fixture
        .errors(owner)
        .contains("system source selection changed during apply"));
    assert_eq!(
        fs::read_link(fixture.path("etc/flake.nix")).unwrap(),
        Path::new("/other/flake.nix")
    );
}

#[test]
fn home_copy_follows_activation_and_failure_does_not_commit_the_record() {
    let mut fixture = Fixture::new();
    fs::write(fixture.path("paths.json"), "[\"managed\"]").unwrap();
    fs::write(fixture.path("source/home/managed"), "frozen").unwrap();
    let owner = fixture.start("missing", "/source/flake.nix");
    wait_for(|| fixture.path("entries").exists());
    assert!(!fixture.path("home/managed").exists());
    assert!(!fixture.path("etc/flake.nix").exists());
    fixture.release();
    assert!(fixture.finish(owner).success(), "{}", fixture.errors(owner));
    assert_eq!(
        fs::read_to_string(fixture.path("home/managed")).unwrap(),
        "frozen"
    );
    assert_eq!(
        fs::metadata(fixture.path("home/managed")).unwrap().uid(),
        unsafe { libc::geteuid() }
    );
    assert_eq!(
        fs::read_to_string(fixture.path("order")).unwrap(),
        "profile\nactivation\n"
    );
    fs::remove_file(fixture.path("source/home/managed")).unwrap();
    fixture.run("/source/flake.nix", "/next/flake.nix", 1);
    assert_eq!(
        fs::read_link(fixture.path("etc/flake.nix")).unwrap(),
        Path::new("/source/flake.nix")
    );
    fs::write(fixture.path("source/home/managed"), "next").unwrap();
    fixture.run("/source/flake.nix", "/next/flake.nix", 0);
}

#[test]
fn copy_only_keeps_the_system_generation_and_checks_it_under_the_shared_lock() {
    let mut fixture = Fixture::new();
    let generation = fixture.path("system").canonicalize().unwrap();
    let current = fixture.path("current-system");
    symlink(&generation, &current).unwrap();
    fs::write(fixture.path("source/home/resource"), "new resource").unwrap();
    fs::write(fixture.path("paths.json"), r#"["resource"]"#).unwrap();
    let options = [
        "--copy-only",
        "--current-generation",
        current.to_str().unwrap(),
    ];
    let contender = fixture.start("missing", "/full/flake.nix");
    wait_for(|| fixture.path("entries").exists());
    let copy = fixture.start_with("missing", "/copy/flake.nix", &options);
    assert!(!fixture.finish(copy).success());
    assert!(fixture
        .errors(copy)
        .contains("system apply is already running"));
    assert!(!fixture.path("home/resource").exists());
    fixture.release();
    assert!(fixture.finish(contender).success());
    fs::remove_file(fixture.path("home/resource")).unwrap();
    fs::remove_file(fixture.path("order")).unwrap();
    fs::remove_file(fixture.path("lix/bin/nix-env")).unwrap();
    fs::remove_file(fixture.path("system/sw/bin/darwin-rebuild")).unwrap();
    let copy = fixture.start_with("/full/flake.nix", "/copy/flake.nix", &options);
    assert!(fixture.finish(copy).success(), "{}", fixture.errors(copy));
    assert_eq!(
        fs::read_to_string(fixture.path("home/resource")).unwrap(),
        "new resource"
    );
    assert_eq!(current.canonicalize().unwrap(), generation);
    assert!(!fixture.path("order").exists());
    assert_eq!(
        fs::read_link(fixture.path("etc/flake.nix")).unwrap(),
        Path::new("/copy/flake.nix")
    );
    fs::write(fixture.path("source/home/resource"), "must not copy").unwrap();
    fs::remove_file(&current).unwrap();
    fs::create_dir(fixture.path("other-system")).unwrap();
    symlink(fixture.path("other-system"), &current).unwrap();
    let copy = fixture.start_with("/copy/flake.nix", "/wrong/flake.nix", &options);
    assert!(!fixture.finish(copy).success());
    assert!(fixture.errors(copy).contains("active generation changed"));
    assert_eq!(
        fs::read_to_string(fixture.path("home/resource")).unwrap(),
        "new resource"
    );
    assert_eq!(
        fs::read_link(fixture.path("etc/flake.nix")).unwrap(),
        Path::new("/copy/flake.nix")
    );
    fs::remove_file(&current).unwrap();
    symlink(&generation, &current).unwrap();
    fs::remove_file(fixture.path("source/home/resource")).unwrap();
    let copy = fixture.start_with("/copy/flake.nix", "/failed/flake.nix", &options);
    assert!(!fixture.finish(copy).success());
    assert_eq!(
        fs::read_link(fixture.path("etc/flake.nix")).unwrap(),
        Path::new("/copy/flake.nix")
    );
}
