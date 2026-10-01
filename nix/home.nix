{ config, configuration, inputs, lib, options, pkgs, dotfilesDirectory, dotfilesSourceHome, ... }:

let
  artifacts = import ./artifacts.nix { inherit configuration pkgs toolset; };
  toolset = import ./packages.nix { inherit inputs pkgs; };
  overlapsCopy = path: builtins.any (owned:
    path == owned || lib.hasPrefix (owned + "/") path || lib.hasPrefix (path + "/") owned
  ) configuration.copy;
  artifactTargets = map (artifact: artifact.homeTarget) (builtins.attrValues artifacts.selected);
  overlapsArtifact = target:
    let path = if target == config.home.homeDirectory then ""
      else lib.removePrefix (config.home.homeDirectory + "/") target;
    in builtins.any (owned:
      path == "" || path == owned || lib.hasPrefix (owned + "/") path || lib.hasPrefix (path + "/") owned
    ) artifactTargets;
  inherit (import ./macos-settings.nix) nightShift;
in
{
  _file = toString ./home.nix;

  home.activation.configureNightShift = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    ${pkgs.nightlight}/bin/nightlight schedule ${nightShift.schedule.start} ${nightShift.schedule.end}
    ${pkgs.nightlight}/bin/nightlight temp ${toString nightShift.temperature}
  '';

  home.stateVersion = "26.05";
  home.packages = toolset.packages ++ [ toolset.dotfiles ];

  assertions = [ {
    assertion = builtins.all (file:
      !file.enable || !(overlapsArtifact file.target)
    ) (builtins.attrValues config.home.file);
    message = "home.file targets conflict with dotfiles artifact paths";
  } {
    assertion = options.home.file.highestPrio == 100 && builtins.all (file:
      !file.enable || !(overlapsCopy file.target)
    ) (builtins.attrValues config.home.file);
    message = "home.file targets conflict with dotfiles copy paths";
  } ];
}
