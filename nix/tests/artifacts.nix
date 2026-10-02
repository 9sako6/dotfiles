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
  nightlight = pkgs.writeShellScriptBin "nightlight" "echo nightlight";
  fixtureToolset = {
    ankiConnect = addon;
    inherit nightlight;
    ffmpeg = throw "mise-managed FFmpeg entered the Nix artifact closure";
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
  samePackage = model: (actual model).selected.localllm.drvPath
    == (toolset.localllm (enabled model).localllm).drvPath;
  results = {
    disabledHasOnlySystemResources = builtins.attrNames off.selected == [ "anki-connect" "nightlight" ];
    disabledRootIsLazy = builtins.stringLength off.root.drvPath > 0;
    nightlightKeepsFixedPackage = (actual "qwen3.8-9b-distill-4bit").selected.nightlight.drvPath
      == (lib.getBin pkgs.nightlight).drvPath;
    publicOutputMatchesConstructor = self.packages.${pkgs.stdenv.hostPlatform.system}.artifacts.drvPath
      == (self.lib.mkArtifacts { }).root.drvPath;
    selected27bUsesFixedPackage = samePackage "qwen3.8-27b-4bit";
    selected9bUsesFixedPackage = samePackage "qwen3.8-9b-distill-4bit";
    systemIndependent = (independent.lib.mkArtifacts { configuration = disabled; }).root.drvPath
      == (self.lib.mkArtifacts { configuration = disabled; }).root.drvPath;
    unrelatedConfigurationIsLazy = (make ((enabled "qwen3.8-27b-4bit") // {
      copy = throw "artifact declarations evaluated copy ownership";
      private = throw "artifact declarations evaluated private configuration";
    }) fixtureToolset).root.drvPath == on.root.drvPath;
  };
in
assert lib.assertMsg (builtins.all (value: value) (builtins.attrValues results))
  "artifact constructor contract test failed";
# Only tiny fixture packages are built. Real LLM packages above are evaluated,
# never interpolated into build inputs, so no model/runtime build is requested.
pkgs.runCommand "artifact-constructor-check" {
  # Expose only the tiny fixture for the registered-root integration test.
  passthru.fixtureRoot = on.root;
} ''
  test -f ${on.root}/share/anki-connect/__init__.py
  test "$(${on.root}/bin/localllm)" = qwen3.8-27b-4bit
  test "$(${on.root}/bin/nightlight)" = nightlight
  test ! -e ${off.root}/bin/localllm
  printf '%s\n' ${lib.escapeShellArg (builtins.toJSON results)} > "$out"
''
