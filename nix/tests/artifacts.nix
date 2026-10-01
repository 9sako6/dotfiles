{ self, pkgs }:
let
  lib = pkgs.lib;
  parse = public: local: (import ../configuration.nix {
    inherit lib;
    publicFile = builtins.toFile "artifact-public.toml" public;
    localFile = builtins.toFile "artifact-local.toml" local;
  }).config;
  enabled = model: parse "copy = []" ''
    [localllm]
    enabled = true
    models = ["${model}"]
    default_model = "${model}"
    [private]
    path = "/unavailable/private-fixture"
  '';
  disabled = parse ''
    [localllm]
    enabled = true
    models = ["qwen3.8-27b-4bit"]
    default_model = "qwen3.8-27b-4bit"
  '' ''
    [localllm]
    enabled = false
    models = []
  '';
  addon = pkgs.writeTextDir "share/anki/addons/anki-connect/__init__.py" "fixture";
  fixtureToolset = {
    ankiConnect = addon;
    localllm = configuration: pkgs.writeShellScriptBin "localllm" ''
      printf '%s\n' '${configuration.default_model}'
    '';
  };
  make = configuration: toolset: import ../artifacts.nix { inherit configuration pkgs toolset; };
  off = make disabled (fixtureToolset // {
    localllm = throw "disabled artifacts evaluated the LLM package";
    localllmClient = throw "disabled artifacts evaluated the client";
    localllmGoalPlugin = throw "disabled artifacts evaluated the plugin";
    localllmRuntime = throw "disabled artifacts evaluated the runtime";
    packages = throw "artifacts evaluated ordinary CLI/GUI packages";
  });
  on = make (enabled "qwen3.8-27b-4bit") fixtureToolset;
  # Re-run the public outputs with unusable system module inputs. The artifact
  # constructor must not force them, even when private configuration is present.
  independent = (import ../../flake.nix).outputs (self.inputs // {
    inherit self;
    home-manager = throw "artifacts evaluated Home Manager";
    nix-darwin = throw "artifacts evaluated nix-darwin";
    nix-homebrew = throw "artifacts evaluated nix-homebrew";
    zundamonotify = throw "artifacts evaluated system service modules";
  });
  toolset = import ../packages.nix { inherit pkgs; inputs = self.inputs; };
  actual = model: independent.lib.mkArtifacts { configuration = enabled model; };
  samePackage = model: (actual model).selected.localllm.package.drvPath
    == (toolset.localllm (enabled model).localllm).drvPath;
  hasReference = package: text: builtins.hasAttr package.drvPath (builtins.getContext text);
  results = {
    disabledHasOnlyAddon = builtins.attrNames off.selected == [ "anki-connect" ]
      && off.homePackages == [ ] && builtins.length off.manifestData.artifacts == 1;
    disabledManifestIsLazy = builtins.stringLength off.manifest.text > 0;
    disabledRootIsLazy = builtins.stringLength off.root.drvPath > 0;
    homeAddonUnchanged = off.homeFiles."Library/Application Support/Anki2/addons21/anki-connect".source
      == "${addon}/share/anki/addons/anki-connect";
    homePackagesSelected = on.homePackages == [ on.selected.localllm.package ];
    manifestKeepsAddonReference = hasReference addon on.manifest.text;
    manifestKeepsLauncherReference = hasReference on.selected.localllm.package on.manifest.text;
    mergedModelSelected = (builtins.elemAt on.manifestData.artifacts 1).model == "qwen3.8-27b-4bit";
    publicOutputMatchesConstructor = self.packages.${pkgs.stdenv.hostPlatform.system}.artifacts.drvPath
      == (self.lib.mkArtifacts { }).root.drvPath;
    selected27bUsesFixedPackage = samePackage "qwen3.8-27b-4bit";
    selected9bUsesFixedPackage = samePackage "qwen3.8-9b-distill-4bit";
    systemIndependent = (independent.lib.mkArtifacts { configuration = disabled; }).root.drvPath
      == (self.lib.mkArtifacts { configuration = disabled; }).root.drvPath;
  };
in
assert lib.assertMsg (builtins.all (value: value) (builtins.attrValues results))
  "artifact constructor contract test failed";
# Only tiny fixture packages are built. Real LLM packages above are evaluated,
# never interpolated into build inputs, so no model/runtime build is requested.
pkgs.runCommand "artifact-constructor-check" { } ''
  test "$(readlink ${on.root}/artifacts/anki-connect)" = ${addon}
  test "$(readlink ${on.root}/artifacts/localllm)" = ${on.selected.localllm.package}
  test "$(readlink ${on.root}/manifest.json)" = ${on.manifest}
  test -f ${on.root}/artifacts/anki-connect/share/anki/addons/anki-connect/__init__.py
  test "$(${on.root}/artifacts/localllm/bin/localllm)" = qwen3.8-27b-4bit
  test ! -e ${off.root}/artifacts/localllm
  printf '%s\n' ${lib.escapeShellArg (builtins.toJSON results)} > "$out"
''
