from pathlib import Path
import shutil


def replace(path, before, after):
    path = Path(path)
    text = path.read_text()
    assert text.count(before) == 1, (path, before[:100], text.count(before))
    path.write_text(text.replace(before, after))


p = Path('cli/src/inventory.rs')
s = p.read_text()
start = s.index('fn skills(source: &Path)')
end = s.index('\n#[cfg(test)]', start)
parser = s[start:end].replace('fn skills(source: &Path)', 'pub fn skills(source: &Path)')
s = s[:start] + s[end:]
start = s.index('#[derive(Clone, PartialEq)]\nstruct Skill')
end = s.index('\npub fn report', start)
s = s[:start] + s[end:]
s = s.replace('use anyhow::{Context, Result};', 'use anyhow::{bail, Context, Result};')
s = s.replace('use std::collections::BTreeMap;\n', 'use crate::agents::Skill;\n')
s = s.replace('    #[serde(skip)]\n    skills:', '    skills:')
s = s.replace('let (mut inventory, source): (Inventory, _) = inputs.load()?;\n    inventory.prepare(&source)?;', 'let (mut inventory, _): (Inventory, _) = inputs.load()?;\n    inventory.prepare();')
s = s.replace('next.prepare(&next.source.clone())?;', 'next.prepare();')
s = s.replace('''        let previous = current.map(read_generation).transpose()?.flatten();
        let notice = if current.is_some() && previous.is_none() {''', '''        let stored = current.map(read_generation_json).transpose()?.flatten();
        let notice = if current.is_some() && stored.is_none() {''')
s = s.replace('''        } else if previous
            .as_ref()
            .is_some_and(|previous| previous.schema_version != next.schema_version)''', '''        } else if stored
            .as_ref()
            .is_some_and(|previous| previous["schemaVersion"].as_u64().unwrap_or(0) != u64::from(next.schema_version))''')
needle = '''        Ok(Self {
            resources: ResourceDiff::between(previous.as_ref(), &next),'''
assert needle in s
s = s.replace(needle, '''        // Decode only compatible payloads, without consulting the old APM tree.
        let previous = if notice.is_none() {
            stored.map(parse_inventory).transpose()?
        } else {
            None
        };
        Ok(Self {
            resources: ResourceDiff::between(previous.as_ref(), &next),''')
s = s.replace('fn read_generation(generation: &Path) -> Result<Option<Inventory>> {', 'fn read_generation_json(generation: &Path) -> Result<Option<Value>> {')
old = '''    let mut inventory: Inventory =
        serde_json::from_slice(&bytes).context("invalid generation inventory")?;
    inventory.prepare(&inventory.source.clone())?;
    Ok(Some(inventory))
}

impl Inventory {
    fn prepare(&mut self, source: &Path) -> Result<()> {'''
new = '''    let value: Value = serde_json::from_slice(&bytes).context("invalid generation inventory")?;
    if !value.is_object() || value.get("schemaVersion").is_some_and(|v| v.as_u64().is_none()) {
        bail!("invalid generation inventory header");
    }
    Ok(Some(value))
}

fn parse_inventory(value: Value) -> Result<Inventory> {
    let mut inventory: Inventory = serde_json::from_value(value).context("invalid generation inventory")?;
    inventory.prepare();
    Ok(inventory)
}

fn read_generation(generation: &Path) -> Result<Option<Inventory>> {
    read_generation_json(generation)?.map(parse_inventory).transpose()
}

impl Inventory {
    fn prepare(&mut self) {'''
assert old in s
s = s.replace(old, new)
s = s.replace('''        self.skills = skills(source)?;
        Ok(())''', '''        self.skills.sort_by(|a, b| a.name.cmp(&b.name));''')
start = s.index('        let skill = source.join("home/.agents/skills/example");')
end = s.index('        fs::write(generation.join("dotfiles-inventory.json")', start)
s = s[:start] + '        fs::create_dir_all(&generation).unwrap();\n' + s[end:]
s = s.replace('''            "source": source,
            "packages":''', '''            "schemaVersion": 4, "source": source,
            "skills": [{"name": "example", "origin": "local", "description": description}],
            "packages":''')
# Keep the compatibility and corruption checks inside the existing unit module.
end = s.rfind('}')
s = s[:end] + '''
    #[test]
    fn legacy_payload_does_not_need_the_current_schema_or_apm_tree() {
        let root = tempfile::tempdir().unwrap();
        let new = generation(root.path(), "new", "2", "Saved description");
        let old = root.path().join("old");
        fs::create_dir(&old).unwrap();
        fs::write(old.join("dotfiles-inventory.json"), r#"{"schemaVersion":3,"obsolete":"legacy representation"}"#).unwrap();
        assert!(Preview::load(Some(&old), &new).unwrap().needs_native());
        let path = new.join("dotfiles-inventory.json");
        let mut broken: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        broken.as_object_mut().unwrap().remove("skills");
        fs::write(&path, serde_json::to_vec(&broken).unwrap()).unwrap();
        assert!(Preview::load(None, &new).is_err());
    }
''' + s[end:]
p.write_text(s)
for name in ['cli/src/inventory/render.rs', 'cli/src/inventory/diff.rs']:
    p = Path(name)
    s = p.read_text().replace('"source": "/fixture",', '"source": "/fixture", "skills": [],')
    p.write_text(s)
p = Path('nix/tests/test_review_terminal.py')
s = p.read_text().replace('"schemaVersion": 2, "source": str(source),', '"schemaVersion": 4, "source": str(source), "skills": [],')
p.write_text(s)

p = Path('cli/src/agents.rs')
s = p.read_text().replace('use serde::Deserialize;', 'use serde::{Deserialize, Serialize};\nuse std::collections::BTreeMap;\nuse std::io::{self, Write};')
addition = '''
#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub struct Skill {
    pub name: String,
    pub origin: String,
    pub description: String,
}

/// Normalize compiled APM resources once while constructing the generation.
pub fn complete_inventory(input: &Path) -> Result<ExitCode> {
    let mut inventory: serde_json::Value = serde_json::from_slice(&fs::read(input)?)?;
    let source = inventory["source"].as_str().context("inventory has no source")?;
    let records = skills(Path::new(source))?;
    let object = inventory.as_object_mut().context("inventory must be an object")?;
    object.insert("schemaVersion".into(), 4.into());
    object.insert("skills".into(), serde_json::to_value(records)?);
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, &inventory)?;
    writeln!(output)?;
    Ok(ExitCode::SUCCESS)
}

''' + parser
s = s.replace('#[cfg(test)]', addition + '\n#[cfg(test)]', 1)
p.write_text(s)
replace('cli/src/main.rs', '''enum Commands {
    #[command(hide = true)]
    CompleteApply''', '''enum Commands {
    #[command(hide = true)]
    CompleteInventory { input: PathBuf },
    #[command(hide = true)]
    CompleteApply''')
replace('cli/src/main.rs', '''    if let Commands::CompleteApply {''', '''    if let Commands::CompleteInventory { input } = command {
        return agents::complete_inventory(&input);
    }
    if let Commands::CompleteApply {''')
replace('cli/src/main.rs', 'Commands::CompleteApply { .. } | Commands::Version => unreachable!(),', 'Commands::CompleteInventory { .. } | Commands::CompleteApply { .. } | Commands::Version => unreachable!(),')
replace('flake.nix', 'inventory = darwinSystem.pkgs.writeText "dotfiles-inventory.json"', 'inventoryInput = darwinSystem.pkgs.writeText "dotfiles-inventory-input.json"')
replace('flake.nix', '''          darwinSystem = nix-darwin.lib.darwinSystem {''', '''          inventory = darwinSystem.pkgs.runCommand "dotfiles-inventory.json" { } ''
            ${dotfilesPackage}/bin/dotfiles complete-inventory ${inventoryInput} > "$out"
          '';
          darwinSystem = nix-darwin.lib.darwinSystem {''')
p = Path('cli/src/system.rs')
s = p.read_text()
start = s.index('    let inventory = evaluate_json(')
end = s.index('    let mut preview =', start)
s = s[:start] + '    let inventory = load_inventory(&nix, host.trim(), show_trace)?;\n' + s[end:]
s = s.replace('pub fn load<T: serde::de::DeserializeOwned>(self) -> Result<(T, PathBuf)>', 'pub fn load(self) -> Result<(crate::inventory::Inventory, PathBuf)>')
start = s.index('        let builds: Vec<serde_json::Value> = evaluate_json(')
end = s.index('        public.verify()?;', start)
block = s[start:end]
s = s[:start] + '        let inventory = load_inventory(&nix, host.trim(), false)?;\n' + s[end:]
helper = block.replace('        ', '    ').replace('nix_command(&nix)', 'nix_command(nix)')
helper = helper.replace('"--no-substitute",\n', '').replace('false,\n', 'show_trace,\n')
addition = '''
fn load_inventory(nix: &Path, host: &str, show_trace: bool) -> Result<crate::inventory::Inventory> {
''' + helper + '    Ok(inventory)\n}\n'
s = s.replace('#[cfg(test)]', addition + '\n#[cfg(test)]', 1)
p.write_text(s)
p = Path('nix/tests/test_inventory.py')
s = p.read_text().replace('self.assertEqual(inventory["schemaVersion"], 3)', 'self.assertEqual(inventory["schemaVersion"], 4)\n        self.assertTrue(any(skill["name"] == "jp" for skill in inventory["skills"]))')
s = s.replace('inventory = host.inventory.text;', 'inventory = host.inventory.drvPath;')
s = s.replace('''            snapshot["inventory"] = json.loads(snapshot["inventory"])''', '''            built = subprocess.run(
                ["nix", "build", "--no-link", "--print-out-paths", snapshot["inventory"] + "^*"],
                capture_output=True, text=True, timeout=180,
            )
            self.assertEqual(built.returncode, 0, built.stderr)
            snapshot["inventory"] = json.loads(Path(built.stdout.strip()).read_text())''')
p.write_text(s)
shutil.copyfile(Path(__file__).with_name('inventory_snapshot.rs'), 'cli/tests/inventory_snapshot.rs')
