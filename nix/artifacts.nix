# The caller supplies the frozen, validated configuration from configuration.nix.
# This constructor does not import or evaluate a Darwin/private module.
{ configuration, pkgs, toolset }:
let
  lib = pkgs.lib;
  candidates = {
    anki-connect = {
      kind = "anki-addon";
      package = toolset.ankiConnect;
      relativePath = "share/anki/addons/anki-connect";
      homeTarget = "Library/Application Support/Anki2/addons21/anki-connect";
    };
    nightlight = {
      kind = "executable";
      package = lib.getBin toolset.nightlight;
      relativePath = "bin/nightlight";
      homeTarget = ".local/bin/nightlight";
    };
  } // lib.optionalAttrs configuration.localllm.enabled {
    localllm = {
      kind = "executable";
      package = toolset.localllm configuration.localllm;
      relativePath = "bin/localllm";
      homeTarget = ".local/bin/localllm";
      model = configuration.localllm.default_model;
    };
  };
  # Keep the complete declaration independent of copy ownership. Rust suppresses
  # overlapping targets in the captured Plan; a copy-only edit can then reuse
  # the same frozen artifact manifest, including when copy ownership is removed.
  selected = candidates;
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
  homePackages = lib.unique (map (artifact: artifact.package)
    (builtins.attrValues (lib.filterAttrs (_: artifact: artifact.kind == "executable") selected)));
}
