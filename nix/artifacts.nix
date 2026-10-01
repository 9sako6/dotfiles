# The caller supplies the frozen, validated configuration from configuration.nix.
# This constructor does not import or evaluate a Darwin/private module.
{ configuration, pkgs, toolset }:
let
  lib = pkgs.lib;
  selected = {
    anki-connect = {
      kind = "anki-addon";
      package = toolset.ankiConnect;
      relativePath = "share/anki/addons/anki-connect";
      homeTarget = "Library/Application Support/Anki2/addons21/anki-connect";
    };
  } // lib.optionalAttrs configuration.localllm.enabled {
    localllm = {
      kind = "executable";
      package = toolset.localllm configuration.localllm;
      relativePath = "bin/localllm";
      model = configuration.localllm.default_model;
    };
  };
  entries = lib.mapAttrsToList (id: artifact:
    (builtins.removeAttrs artifact [ "package" ]) // {
      inherit id;
      storePath = toString artifact.package;
    }
  ) selected;
  manifestData = {
    schemaVersion = 1;
    artifacts = entries;
  };
  # Keep Nix string context: both the manifest and root retain the store closure.
  manifest = pkgs.writeText "dotfiles-artifacts.json" (builtins.toJSON manifestData);
  root = pkgs.linkFarm "dotfiles-artifacts" (
    [ { name = "manifest.json"; path = manifest; } ]
    ++ lib.mapAttrsToList (id: artifact: {
      name = "artifacts/${id}";
      path = artifact.package;
    }) selected
  );
in
{
  inherit manifest manifestData root selected;
  homeFiles = builtins.listToAttrs (lib.mapAttrsToList (_: artifact: {
    name = artifact.homeTarget;
    value.source = "${artifact.package}/${artifact.relativePath}";
  }) (lib.filterAttrs (_: artifact: artifact ? homeTarget) selected));
  homePackages = map (artifact: artifact.package)
    (builtins.attrValues (lib.filterAttrs (_: artifact: artifact.kind == "executable") selected));
}
