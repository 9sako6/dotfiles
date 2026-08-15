use std::path::Path;
use std::process::{Command, ExitCode};

use anyhow::{bail, Context, Result};

trait CommandRunner {
    fn run(&self, command: &str, args: &[&str], cwd: &Path) -> Result<()>;
}

struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run(&self, command: &str, args: &[&str], cwd: &Path) -> Result<()> {
        println!("$ {command} {}", args.join(" "));
        let status = Command::new(command)
            .args(args)
            .current_dir(cwd)
            .status()
            .with_context(|| format!("failed to execute {command}"))?;
        if status.success() {
            return Ok(());
        }
        bail!(
            "{command} exited with status {}",
            status
                .code()
                .map_or_else(|| "signal".to_owned(), |code| code.to_string())
        )
    }
}

pub fn run<F>(repo_root: &Path, validate_plan: F) -> Result<ExitCode>
where
    F: FnOnce(&Path) -> Result<ExitCode>,
{
    run_with(repo_root, &ProcessRunner, validate_plan)
}

fn run_with<R, F>(repo_root: &Path, runner: &R, validate_plan: F) -> Result<ExitCode>
where
    R: CommandRunner,
    F: FnOnce(&Path) -> Result<ExitCode>,
{
    println!("==> update public nix-darwin and Home Manager inputs");
    let flake_path = repo_root.display().to_string();
    runner.run(
        "nix",
        &["flake", "update", "--flake", flake_path.as_str()],
        repo_root,
    )?;
    println!("==> validate public system plan");
    validate_plan(repo_root)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::Path;

    use super::{run_with, CommandRunner};

    #[derive(Default)]
    struct FakeRunner {
        calls: RefCell<Vec<(String, Vec<String>, String)>>,
        fail: bool,
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, command: &str, args: &[&str], cwd: &Path) -> anyhow::Result<()> {
            self.calls.borrow_mut().push((
                command.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect(),
                cwd.display().to_string(),
            ));
            if self.fail {
                anyhow::bail!("update failed")
            }
            Ok(())
        }
    }

    #[test]
    fn updates_before_validating_the_public_plan() {
        let runner = FakeRunner::default();
        let plan_calls = RefCell::new(Vec::new());
        let temp = tempfile::tempdir().unwrap();
        run_with(temp.path(), &runner, |repo| {
            plan_calls.borrow_mut().push(repo.to_path_buf());
            Ok(std::process::ExitCode::SUCCESS)
        })
        .unwrap();

        assert_eq!(runner.calls.borrow().len(), 1);
        assert_eq!(runner.calls.borrow()[0].0, "nix");
        assert_eq!(
            runner.calls.borrow()[0].1[0..3],
            ["flake", "update", "--flake"]
        );
        assert_eq!(plan_calls.borrow().as_slice(), &[temp.path().to_path_buf()]);
    }

    #[test]
    fn does_not_validate_when_update_fails() {
        let runner = FakeRunner {
            fail: true,
            ..FakeRunner::default()
        };
        let plan_called = RefCell::new(false);
        let temp = tempfile::tempdir().unwrap();
        assert!(run_with(temp.path(), &runner, |_| {
            *plan_called.borrow_mut() = true;
            Ok(std::process::ExitCode::SUCCESS)
        })
        .is_err());
        assert!(!*plan_called.borrow());
    }
}
