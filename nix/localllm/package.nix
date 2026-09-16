{ configuration, inputs, pkgs }:
let
  catalog = import ./catalog.nix;
  model = import ./model.nix { inherit pkgs; model = catalog.${configuration.default_model}; };
  runtime = (import ./runtime.nix { inherit inputs pkgs; }).environment;
  opencode = pkgs.opencode;
  goalPlugin = import ./goal-plugin.nix { inherit pkgs; };
  settings = pkgs.writeText "localllm-launcher.json" (builtins.toJSON {
    inherit model;
    goal_plugin = "${goalPlugin}";
    opencode = "${opencode}/bin/opencode";
    python = "${runtime}/bin/python";
  });
in
assert pkgs.lib.assertMsg (opencode.version == "1.18.13") "OpenCode version drifted from 1.18.13";
pkgs.writeShellScriptBin "localllm" ''
  exec ${runtime}/bin/python ${./launcher.py} --settings ${settings} "$@"
''
