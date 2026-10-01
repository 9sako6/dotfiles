//! Standalone behavior coverage while the orchestration adopts these helpers.
#[path = "../src/system/configuration.rs"]
mod configuration;
#[path = "../src/system/snapshot.rs"]
mod snapshot;

#[test]
#[ignore = "requires Nix and pinned flake inputs; run explicitly on the macOS CI runner"]
fn rust_configuration_matches_the_nix_schema() {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let catalog: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_slice(&fs::read(repository.join("nix/localllm/catalog.json")).unwrap())
            .unwrap();
    let models: Vec<_> = catalog.into_keys().collect();
    let model = models.first().expect("model catalog must not be empty");
    let enabled =
        format!("[localllm]\nenabled = true\nmodels = ['{model}']\ndefault_model = '{model}'\n");
    let mut cases: Vec<(String, Option<String>)> = vec![
        (String::new(), None),
        ("copy = ['a', 'b/c']".into(), None),
        (enabled.clone(), None),
        (enabled.clone(), Some("[localllm]\nenabled = false".into())),
        (
            enabled,
            Some("[localllm]\nenabled = false\nmodels = []\n[private]\npath = '../private'".into()),
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
            configuration::Configuration::parse(
                public.as_bytes(),
                local.as_deref().map(str::as_bytes),
                &models,
            )
            .ok()
            .map(|configuration| serde_json::to_value(configuration).unwrap())
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
    let actual: Vec<Option<serde_json::Value>> = serde_json::from_slice(&output.stdout).unwrap();
    for (index, (expected, actual)) in expected.iter().zip(actual.iter()).enumerate() {
        assert_eq!(expected, actual, "configuration case {index}");
    }
    assert_eq!(actual.len(), expected.len());
}
