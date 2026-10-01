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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Configuration {
    pub copy: Vec<String>,
    pub localllm: LocalLlm,
    pub private: Private,
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
            "localllm" | "private" => Some(value.is_table()),
            "localllm.enabled" => Some(value.is_bool()),
            "localllm.default_model" | "private.path" => Some(value.is_str()),
            _ => None,
        };
        match valid {
            None => errors.push(format!("{file}: {path}: unknown key")),
            Some(false) => errors.push(format!("{file}: {path}: invalid type")),
            Some(true) if path == "localllm" || path == "private" => {
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
        ] {
            let error = format!("{:#}", parse(public, local).unwrap_err());
            assert!(!error.contains("private-secret"));
        }
        let error = parse("", Some("[localllm]\nmodels = ['unknown']"))
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("dotfiles.local.toml: localllm.models:"));
    }
}
