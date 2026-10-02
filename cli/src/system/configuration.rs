//! Nix-free validation of the public/local configuration contract.
//!
//! Model identifiers come from the caller's canonical catalog, never a second
//! hard-coded list. The Nix boundary still validates this contract independently.
#[cfg(test)]
use std::fs;
use std::path::Path;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use toml::Value;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Configuration {
    pub copy: Vec<String>,
    pub localllm: LocalLlm,
    pub private: Private,
    pub services: super::user_services::Declarations,
    pub settings: super::user_settings::Declarations,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct LocalLlm {
    pub default_model: Option<String>,
    pub enabled: bool,
    pub models: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Private {
    pub path: Option<String>,
}

impl Configuration {
    pub fn system_value(&self) -> serde_json::Value {
        serde_json::json!({"copy": self.copy, "localllm": self.localllm, "private": self.private})
    }

    pub fn load(source: &Path, local: Option<&[u8]>) -> Result<Self> {
        let public =
            super::snapshot::read_regular(source, Path::new("dotfiles.toml"), "dotfiles.toml")?;
        let catalog: std::collections::BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(&super::snapshot::read_regular(
                source,
                Path::new("nix/localllm/catalog.json"),
                "model catalog",
            )?)
            .map_err(|_| anyhow::anyhow!("invalid model catalog"))?;
        if catalog.values().any(|entry| !entry.is_object()) {
            bail!("invalid model catalog");
        }
        Self::parse(&public, local, &catalog.into_keys().collect::<Vec<_>>())
    }

    pub fn parse(public: &[u8], local: Option<&[u8]>, known_models: &[String]) -> Result<Self> {
        let public = parse_file(public, "dotfiles.toml")?;
        let local = local
            .map(|bytes| parse_file(bytes, "dotfiles.local.toml"))
            .transpose()?
            .unwrap_or_else(|| Value::Table(Default::default()));
        let mut errors = Vec::new();
        validate_structure(&public, "dotfiles.toml", "", &mut errors);
        validate_structure(&local, "dotfiles.local.toml", "", &mut errors);
        if public.get("private").is_some() {
            errors.push("dotfiles.toml: private: only allowed in dotfiles.local.toml".into());
        }
        if local.get("copy").is_some() {
            errors.push("dotfiles.local.toml: copy: only allowed in dotfiles.toml".into());
        }
        if !errors.is_empty() {
            bail!("{}", errors.join("\n"));
        }
        let mut merged = public;
        merge(&mut merged, &local);
        let configuration: Self = merged
            .try_into()
            .map_err(|_| anyhow::anyhow!("configuration does not match its validated schema"))?;
        let copy = &configuration.copy;
        if !sorted_unique(copy) {
            errors.push("dotfiles.toml: copy: entries must be unique and alphabetical".into());
        }
        if !copy.iter().all(|path| valid_relative_path(path)) {
            errors.push("dotfiles.toml: copy: invalid relative path".into());
        }
        if copy.iter().any(|a| {
            copy.iter()
                .any(|b| a != b && b.starts_with(&format!("{a}/")))
        }) {
            errors.push("dotfiles.toml: copy: entries must not overlap".into());
        }
        let owner = |key: &str| {
            if local.get("localllm").and_then(|llm| llm.get(key)).is_some() {
                "dotfiles.local.toml"
            } else {
                "dotfiles.toml"
            }
        };
        let llm = &configuration.localllm;
        if !sorted_unique(&llm.models) {
            errors.push(format!(
                "{}: localllm.models: entries must be unique and alphabetical",
                owner("models")
            ));
        }
        if !llm.models.iter().all(|model| known_models.contains(model)) {
            errors.push(format!(
                "{}: localllm.models: unknown model identifier",
                owner("models")
            ));
        }
        if llm.enabled && llm.models.is_empty() {
            errors.push(format!(
                "{}: localllm.models: enabled requires a model",
                owner("enabled")
            ));
        }
        if llm.enabled && llm.models.len() != 1 {
            errors.push(format!(
                "{}: localllm.models: only one loaded model is supported",
                owner("models")
            ));
        }
        if llm.enabled
            && !llm
                .default_model
                .as_ref()
                .is_some_and(|model| llm.models.contains(model))
        {
            errors.push(format!(
                "{}: localllm.default_model: must belong to models",
                owner("default_model")
            ));
        }
        if configuration.private.path.as_deref() == Some("") {
            errors.push("dotfiles.local.toml: private.path: must not be empty".into());
        }
        if !errors.is_empty() {
            bail!("{}", errors.join("\n"));
        }
        configuration.services.validate()?;
        configuration.settings.validate()?;
        Ok(configuration)
    }
}

fn parse_file(bytes: &[u8], file: &str) -> Result<Value> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("{file}: invalid TOML or unreadable configuration"))?;
    toml::from_str(text)
        .map_err(|_| anyhow::anyhow!("{file}: invalid TOML or unreadable configuration"))
}

fn validate_structure(value: &Value, file: &str, prefix: &str, errors: &mut Vec<String>) {
    let Some(table) = value.as_table() else {
        errors.push(format!("{file}: configuration: invalid type"));
        return;
    };
    for (key, value) in table {
        let path = format!("{prefix}{key}");
        let valid = match path.as_str() {
            "copy" | "localllm.models" => Some(
                value
                    .as_array()
                    .is_some_and(|values| values.iter().all(Value::is_str)),
            ),
            "localllm" | "private" | "settings" | "settings.night_shift" => Some(value.is_table()),
            "services" => Some(
                value.is_table()
                    && value
                        .clone()
                        .try_into::<super::user_services::Declarations>()
                        .is_ok(),
            ),
            "localllm.enabled" => Some(value.is_bool()),
            "localllm.default_model"
            | "private.path"
            | "settings.night_shift.start"
            | "settings.night_shift.end" => Some(value.is_str()),
            "settings.night_shift.temperature" => Some(value.is_integer()),
            _ => None,
        };
        match valid {
            None => errors.push(format!("{file}: {path}: unknown key")),
            Some(false) => errors.push(format!("{file}: {path}: invalid type")),
            Some(true) if value.is_table() && path != "services" => {
                validate_structure(value, file, &format!("{path}."), errors);
            }
            Some(true) => (),
        }
    }
}

fn merge(target: &mut Value, overriding: &Value) {
    if let (Some(target), Some(overriding)) = (target.as_table_mut(), overriding.as_table()) {
        for (key, value) in overriding {
            if let Some(existing) = target.get_mut(key) {
                merge(existing, value);
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    } else {
        *target = overriding.clone();
    }
}

fn sorted_unique(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn valid_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(public: &str, local: Option<&str>) -> Result<Configuration> {
        Configuration::parse(
            public.as_bytes(),
            local.map(str::as_bytes),
            &["model-a".into(), "model-b".into()],
        )
    }

    #[test]
    fn loads_model_identifiers_from_the_canonical_catalog() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("nix/localllm")).unwrap();
        fs::write(
            root.path().join("nix/localllm/catalog.json"),
            r#"{"test-model":{}}"#,
        )
        .unwrap();
        fs::write(
            root.path().join("dotfiles.toml"),
            "[localllm]\nmodels = ['test-model']\n",
        )
        .unwrap();
        assert_eq!(
            Configuration::load(root.path(), None)
                .unwrap()
                .localllm
                .models,
            ["test-model"]
        );
        fs::write(
            root.path().join("nix/localllm/catalog.json"),
            r#"{"other-model":{}}"#,
        )
        .unwrap();
        assert!(Configuration::load(root.path(), None).is_err());
    }

    #[test]
    fn configuration_and_catalog_symlinks_cannot_escape_the_frozen_source() {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret");
        fs::write(&secret, "secret-do-not-print").unwrap();
        for relative in ["dotfiles.toml", "nix/localllm/catalog.json"] {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir_all(root.path().join("nix/localllm")).unwrap();
            fs::write(root.path().join("dotfiles.toml"), "").unwrap();
            fs::write(root.path().join("nix/localllm/catalog.json"), "{}").unwrap();
            fs::remove_file(root.path().join(relative)).unwrap();
            symlink(&secret, root.path().join(relative)).unwrap();
            let error = Configuration::load(root.path(), None).unwrap_err();
            assert!(!format!("{error:#}").contains("secret-do-not-print"));
            assert_eq!(fs::read_to_string(&secret).unwrap(), "secret-do-not-print");
        }
    }

    #[test]
    fn defaults_and_recursive_overrides_match_the_configuration_contract() {
        assert_eq!(parse("", None).unwrap(), Configuration::default());
        let configuration = parse(
            "copy = ['.config/tool', '.gitconfig']\n[localllm]\nenabled = true\nmodels = ['model-a']\ndefault_model = 'model-a'\n",
            Some("[localllm]\nenabled = false\nmodels = []\n[private]\npath = '../private'\n"),
        ).unwrap();
        assert_eq!(configuration.copy, [".config/tool", ".gitconfig"]);
        assert!(!configuration.localllm.enabled);
        assert!(configuration.localllm.models.is_empty());
        assert_eq!(
            configuration.localllm.default_model.as_deref(),
            Some("model-a")
        );
        assert_eq!(configuration.private.path.as_deref(), Some("../private"));
    }

    #[test]
    fn rootless_sections_merge_locally_without_changing_the_system_contract() {
        let public = "[[services.agents]]\nlabel = 'com.example.check'\nargv = ['/usr/bin/true']\n[settings.night_shift]\nstart = '21:00'\nend = '06:00'\ntemperature = 60\n";
        let original = parse(public, Some("[private]\npath = '../private'\n")).unwrap();
        let overridden = parse(public, Some("[private]\npath = '../private'\n[services]\nagents = []\n[settings.night_shift]\ntemperature = 75\n")).unwrap();
        assert_eq!(original.system_value(), overridden.system_value());
        assert_eq!(
            super::super::user_services::declared_inventory(&original.services)
                .unwrap()
                .len(),
            1
        );
        assert!(
            super::super::user_services::declared_inventory(&overridden.services)
                .unwrap()
                .is_empty()
        );
        let preferences =
            super::super::user_settings::declared_inventory(&overridden.settings).unwrap();
        assert_eq!(preferences[0]["start"], "21:00");
        assert_eq!(preferences[0]["end"], "06:00");
        assert_eq!(preferences[0]["temperature"], 75);
    }

    #[test]
    fn rejects_unknown_keys_types_and_source_ownership() {
        for public in [
            "unknown = true",
            "_module = {}",
            "copy = [1]",
            "copy = 'path'",
            "localllm = false",
            "[localllm]\nenabled = 1",
            "[localllm]\nmodels = [false]",
            "[localllm]\nextra = true",
            "[localllm]\ndefault_model = []",
            "[private]",
            "services = []",
            "[services]\nextra = 'private-secret'",
            "settings = []",
            "[settings]\nextra = 'private-secret'",
            "[settings.night_shift]\nstart = '21:00'",
            "[settings.night_shift]\nstart = '21:00'\nend = '06:00'\ntemperature = 60\nextra = 'private-secret'",
        ] {
            assert!(parse(public, None).is_err(), "{public}");
        }
        for local in [
            "copy = []",
            "unknown = true",
            "private = false",
            "[private]\npath = 1",
            "[private]\nextra = true",
            "[private]\npath = ''",
        ] {
            assert!(parse("", Some(local)).is_err(), "{local}");
        }
    }

    #[test]
    fn rejects_invalid_copy_and_model_values() {
        for paths in [
            "['b', 'a']",
            "['a', 'a']",
            "['a', 'a/b']",
            "['']",
            "['/a']",
            "['a/../b']",
            "['a/./b']",
            "['a//b']",
            "['a/']",
        ] {
            assert!(parse(&format!("copy = {paths}"), None).is_err(), "{paths}");
        }
        for llm in [
            "models = ['unknown']",
            "models = ['model-b', 'model-a']",
            "models = ['model-a', 'model-a']",
            "enabled = true",
            "enabled = true\nmodels = ['model-a']",
            "enabled = true\nmodels = ['model-a']\ndefault_model = 'model-b'",
            "enabled = true\nmodels = ['model-a', 'model-b']\ndefault_model = 'model-a'",
        ] {
            assert!(parse(&format!("[localllm]\n{llm}"), None).is_err(), "{llm}");
        }
        assert!(parse(
            "[localllm]\nenabled = true\nmodels = ['model-a']\ndefault_model = 'model-a'",
            None
        )
        .is_ok());
    }

    #[test]
    fn diagnostics_never_include_configuration_values() {
        for (public, local) in [
            ("copy = ['private-secret'", None),
            ("", Some("[private]\npath = ['private-secret']")),
            ("[localllm]\nmodels = ['private-secret']", None),
            (
                "[[services.agents]]\nlabel = 'com.example.check'\nargv = ['private-secret']",
                None,
            ),
            (
                "[settings.night_shift]\nstart = 'private-secret'\nend = '06:00'\ntemperature = 60",
                None,
            ),
        ] {
            let error = format!("{:#}", parse(public, local).unwrap_err());
            assert!(!error.contains("private-secret"));
        }
        let error = parse("", Some("[localllm]\nmodels = ['unknown']"))
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("dotfiles.local.toml: localllm.models:"));
    }
    #[test]
    #[ignore = "requires Nix and pinned flake inputs; run explicitly on the macOS CI runner"]
    fn rust_configuration_matches_the_nix_schema() {
        use std::fs;
        use std::path::Path;
        use std::process::Command;

        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let catalog: std::collections::BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(
                &fs::read(repository.join("nix/localllm/catalog.json")).unwrap(),
            )
            .unwrap();
        let models: Vec<_> = catalog.into_keys().collect();
        let model = models.first().expect("model catalog must not be empty");
        let enabled = format!(
            "[localllm]\nenabled = true\nmodels = ['{model}']\ndefault_model = '{model}'\n"
        );
        let mut cases: Vec<(String, Option<String>)> = vec![
            (String::new(), None),
            ("copy = ['a', 'b/c']".into(), None),
            (
                "[settings.night_shift]\nstart = '21:00'\nend = '06:00'\ntemperature = 60".into(),
                Some("[settings.night_shift]\ntemperature = 75".into()),
            ),
            (
                "[[services.agents]]\nlabel = 'com.example.check'\nargv = ['/usr/bin/true']".into(),
                Some("[services]\nagents = []".into()),
            ),
            (enabled.clone(), None),
            (enabled.clone(), Some("[localllm]\nenabled = false".into())),
            (
                enabled,
                Some(
                    "[localllm]\nenabled = false\nmodels = []\n[private]\npath = '../private'"
                        .into(),
                ),
            ),
        ];
        for public in [
            "copy = []",
            "copy = [1]",
            "copy = ['/absolute']",
            "copy = ['a', 'a/b']",
            "copy = ['a/', 'b']",
            "copy = ['b', 'a']",
            "copy = ['a', 'a']",
            "copy = ['a/../b']",
            "copy = ['a/./b']",
            "copy = ['a//b']",
            "copy = ['']",
            "copy = 'a'",
            "[private]\npath = '../private'",
            "[_module]\nargs = {}",
            "[localllm._module]\nargs = {}",
            "unknown = 'redacted-value'",
            "[localllm]\nenabled = 2",
            "[localllm]\nmodels = [2]",
            "[localllm]\ndefault_model = false",
            "[localllm]\nenabled = true",
            "[localllm]\nmodels = ['unknown']",
        ] {
            cases.push((public.into(), None));
        }
        for local in [
            "copy = []",
            "[private]\npath = 2",
            "[private]\npath = ''",
            "[private]\nextra = 'value'",
            "[localllm]\nenabld = true",
            "localllm = false",
        ] {
            cases.push((String::new(), Some(local.into())));
        }
        let expected: Vec<Option<serde_json::Value>> = cases
            .iter()
            .map(|(public, local)| {
                Configuration::parse(
                    public.as_bytes(),
                    local.as_deref().map(str::as_bytes),
                    &models,
                )
                .ok()
                .map(|configuration| configuration.system_value())
            })
            .collect();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cases.json");
        fs::write(&path, serde_json::to_vec(&cases).unwrap()).unwrap();
        let output = Command::new("nix")
            .args([
                "--extra-experimental-features",
                "nix-command flakes",
                "eval",
                "--impure",
                "--json",
                "--no-write-lock-file",
                "--no-update-lock-file",
                "--expr",
                r#"let
                  root = builtins.getEnv "DOTFILES_CONFIGURATION_TEST_ROOT";
                  source = builtins.getFlake ("path:" + root);
                  cases = builtins.fromJSON (builtins.readFile (builtins.getEnv "DOTFILES_CONFIGURATION_TEST_CASES"));
                in map (pair:
                  let
                    local = builtins.elemAt pair 1;
                    parsed = import (root + "/nix/configuration.nix") {
                      inherit (source.inputs.nixpkgs) lib;
                      publicFile = builtins.toFile "dotfiles.toml" (builtins.elemAt pair 0);
                      localFile = if local == null then null else builtins.toFile "dotfiles.local.toml" local;
                    };
                  in if parsed.errors == [] then parsed.config else null
                ) cases"#,
            ])
            .env("DOTFILES_CONFIGURATION_TEST_ROOT", repository)
            .env("DOTFILES_CONFIGURATION_TEST_CASES", path)
            .output()
            .expect("Nix must be installed for the explicit parity test");
        assert!(
            output.status.success(),
            "Nix schema comparison failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: Vec<Option<serde_json::Value>> =
            serde_json::from_slice(&output.stdout).unwrap();
        for (index, (expected, actual)) in expected.iter().zip(actual.iter()).enumerate() {
            assert_eq!(expected, actual, "configuration case {index}");
        }
        assert_eq!(actual.len(), expected.len());
    }
}
