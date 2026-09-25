use std::env;
use std::ffi::{CString, OsString};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{symlink, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus};

use anyhow::{bail, Context, Result};

#[derive(clap::Args, Debug)]
pub struct Args {
    #[arg(long)]
    copy_only: bool,
    #[arg(long, default_value = "/run/current-system")]
    current_generation: PathBuf,
    nix: PathBuf,
    user: String,
    system: PathBuf,
    selection: PathBuf,
    expected: PathBuf,
    desired: PathBuf,
    source: PathBuf,
    paths: PathBuf,
    home: PathBuf,
}

pub fn run(args: Args) -> Result<ExitCode> {
    let nix_env = args.nix.with_file_name("nix-env");
    let rebuild = args.system.join("sw/bin/darwin-rebuild");
    if !args.copy_only {
        require_executable(&nix_env).context("built Lix has no nix-env")?;
        require_executable(&rebuild).context("built system has no darwin-rebuild")?;
    }
    let directory = args
        .selection
        .parent()
        .context("source record has no parent")?;
    if !args.copy_only {
        fs::create_dir_all(directory)?;
    }
    let mut lock_path = OsString::from(&args.selection);
    lock_path.push(".apply.lock");
    let _lock = acquire_lock(Path::new(&lock_path), args.copy_only)?;
    verify_record(&args)?;
    verify_generation(&args)?;
    if args.copy_only && (args.expected == Path::new("missing") || args.expected != args.desired) {
        bail!("home copy requires an unchanged source record; run a normal system apply first");
    }

    let mut commands = Vec::new();
    if !args.copy_only {
        commands.extend([
            {
                let mut command = Command::new(nix_env);
                command.args(["-p", "/nix/var/nix/profiles/system", "--set"]);
                command.arg(&args.system);
                command
            },
            {
                let mut command = Command::new(rebuild);
                command.arg("activate");
                command
            },
        ]);
    }
    commands.push({
        let mut command = Command::new(env::current_exe()?);
        command.args(["complete-apply", "--user", &args.user]);
        command.args([&args.source, &args.paths, &args.home]);
        command.env("HOME", &args.home);
        command.env("USER", &args.user).env("LOGNAME", &args.user);
        command
    });
    for mut command in commands {
        let status = command
            .env("SUDO_USER", &args.user)
            .status()
            .context("cannot start system activation or home copy")?;
        if !status.success() {
            return Ok(exit_code(status));
        }
    }

    verify_record(&args)?;
    verify_generation(&args)?;
    if args.copy_only {
        return Ok(ExitCode::SUCCESS);
    }
    let staging = tempfile::Builder::new()
        .prefix(".dotfiles-record-")
        .tempdir_in(directory)?;
    let link = staging.path().join("flake.nix");
    symlink(&args.desired, &link)?;
    fs::rename(&link, &args.selection).context("could not persist system source selection")?;
    Ok(ExitCode::SUCCESS)
}

fn verify_generation(args: &Args) -> Result<()> {
    if args.copy_only && args.current_generation.canonicalize().ok().as_ref() != Some(&args.system)
    {
        bail!("active generation changed during copy; the source record is retained");
    }
    Ok(())
}

fn acquire_lock(path: &Path, copy_only: bool) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(!copy_only)
        .create(!copy_only)
        .truncate(false)
        .mode(0o644)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .context(if copy_only {
            "cannot open system apply lock; run a normal system apply once to prepare password-free home copy"
        } else {
            "cannot open system apply lock"
        })?;
    if !file.metadata()?.is_file() {
        bail!("system apply lock is not a regular file");
    }
    fs2::FileExt::try_lock_exclusive(&file).context("system apply is already running")?;
    if !copy_only {
        file.set_permissions(fs::Permissions::from_mode(0o644))?;
    }
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFD) };
    if flags < 0
        || unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, flags & !libc::FD_CLOEXEC) } < 0
    {
        return Err(io::Error::last_os_error()).context("cannot retain system apply lock");
    }
    Ok(file)
}

fn verify_record(args: &Args) -> Result<()> {
    let matches = match fs::symlink_metadata(&args.selection) {
        Ok(metadata) => {
            args.expected != Path::new("missing")
                && metadata.file_type().is_symlink()
                && fs::read_link(&args.selection)? == args.expected
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            args.expected == Path::new("missing")
        }
        Err(error) => return Err(error).context("cannot inspect system source selection"),
    };
    if !matches {
        bail!("system source selection changed during apply");
    }
    Ok(())
}

fn require_executable(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        bail!("not an executable file: {}", path.display());
    }
    Ok(())
}

fn exit_code(status: ExitStatus) -> ExitCode {
    ExitCode::from(
        status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)) as u8,
    )
}

pub fn become_user(user: &str) -> Result<()> {
    let name = CString::new(user).context("invalid copy user")?;
    let mut buffer = vec![0; 16384];
    let mut entry = MaybeUninit::<libc::passwd>::uninit();
    let mut result = std::ptr::null_mut();
    loop {
        let error = unsafe {
            libc::getpwnam_r(
                name.as_ptr(),
                entry.as_mut_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut result,
            )
        };
        if error == libc::ERANGE {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error)).context("cannot resolve copy user");
        }
        if result.is_null() {
            bail!("copy user does not exist");
        }
        break;
    }
    let entry = unsafe { entry.assume_init() };
    if unsafe { libc::geteuid() } == 0 {
        if unsafe { libc::initgroups(name.as_ptr(), entry.pw_gid as _) } != 0
            || unsafe { libc::setgid(entry.pw_gid) } != 0
            || unsafe { libc::setuid(entry.pw_uid) } != 0
        {
            return Err(io::Error::last_os_error()).context("cannot drop home copy privileges");
        }
    } else if unsafe { libc::geteuid() } != entry.pw_uid {
        bail!("home copy must run as the selected user");
    }
    Ok(())
}
