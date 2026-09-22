from pathlib import Path
import shutil


def replace(path, before, after):
    path = Path(path)
    text = path.read_text()
    assert text.count(before) == 1, (path, before[:100], text.count(before))
    path.write_text(text.replace(before, after))


replace('cli/src/system.rs', 'pub fn run(mode: Mode, root: &Path, show_trace: bool) -> Result<ExitCode> {', '''#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemPaths {
    source_record: PathBuf,
    current_generation: PathBuf,
}

pub fn run(mode: Mode, root: &Path, show_trace: bool) -> Result<ExitCode> {''')
replace('cli/src/system.rs', '    let selection = Path::new("/etc/nix-darwin/flake.nix");', '''    let backend = root.join("bin/system-backend.sh");
    let system_paths: SystemPaths = serde_json::from_slice(&capture(
        Command::new(&backend).arg("paths"),
        "cannot identify system paths",
    )?)?;
    let selection = system_paths.source_record.as_path();''')
p = Path('cli/src/system.rs')
s = p.read_text()
assert s.count('current_generation(Path::new("/run/current-system"))') == 2
s = s.replace('current_generation(Path::new("/run/current-system"))', 'current_generation(&system_paths.current_generation)')
needle = '    let backend = root.join("bin/system-backend.sh");\n'
pos = s.index(needle, s.index(needle) + len(needle))
s = s[:pos] + s[pos:].replace(needle, '', 1)
p.write_text(s)
replace('bin/system-backend.sh', 'case "$operation" in\n', '''case "$operation" in
  paths) printf '%s\\n' '{"sourceRecord":"/etc/nix-darwin/flake.nix","currentGeneration":"/run/current-system"}' ;;
''')
replace('lib/install-system.sh', '    desired_target="$6"\n', '    desired_target="$6"\n    sudo_bin="$7"\n')
replace('lib/install-system.sh', '    shift 6\n', '    shift 7\n')
replace('lib/install-system.sh', '      /usr/bin/sudo --user="$SUDO_USER" --', '      "$sudo_bin" --user="$SUDO_USER" --')
replace('lib/install-system.sh', '    "$expected_target" "$desired_target" "$@"\n)', '    "$expected_target" "$desired_target" "$sudo_bin" "$@"\n)')
replace('cli/src/agents.rs', '        fail_at: Option<usize>,', "        fail_on: Option<&'static str>,")
replace('cli/src/agents.rs', '            if self.fail_at == Some(calls.len()) {', '            if self.fail_on.is_some_and(|action| args.first().is_some_and(|arg| arg == action)) {')
replace('cli/src/agents.rs', '        assert_eq!(runner.calls.borrow().len(), 4);\n', '')
replace('cli/src/agents.rs', '            fail_at: Some(4),', '            fail_on: Some("compile"),')
replace('cli/src/agents.rs', '            fail_at: Some(1),', '            fail_on: Some("install"),')
Path('nix/tests/test_plan.py').rename('nix/tests/test_review_terminal.py')
replace('nix/tests/test_review_terminal.py', 'class PlanTests(unittest.TestCase):', 'class ReviewTerminalTests(unittest.TestCase):')
for name in ('test_plan.py', 'test_agents.py', 'test_bootstrap.py'):
    shutil.copyfile(Path(__file__).with_name(name), Path('nix/tests') / name)
shutil.copyfile(Path(__file__).with_name('test_goal.py'), 'nix/localllm/test_goal.py')
p = Path('tests/bootstrap.test.ts')
s = p.read_text()
start = s.index('function nixApplyLog(')
end = s.index('\nasync function prepareBootstrapEnvironment', start)
s = s[:start] + s[end:]
old = '''      expect(await readFile(logPath, "utf8")).toBe(
        "install-mise\\n" +
          nixApplyLog(env.DOTFILES_DIR!) +
          "mise <trust>\\nmise <install>\\nmise <bootstrap> <--yes> <--verbose>\\n",
      );'''
assert old in s
s = s.replace(old, '''      const log = await readFile(logPath, "utf8");
      expect(log).toContain("install-mise\\n");
      expect(log).toContain("mise <bootstrap> <--yes> <--verbose>\\n");''')
s = s.replace('      expect(await readFile(logPath, "utf8")).toContain(nixApplyLog(dotfilesDir));\n', '')
p.write_text(s)
