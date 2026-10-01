{ config, configuration, inputs, lib, options, pkgs, dotfilesDirectory, dotfilesSourceHome, ... }:

let
  artifacts = import ./artifacts.nix { inherit configuration pkgs toolset; };
  toolset = import ./packages.nix { inherit inputs pkgs; };
  overlapsCopy = path: builtins.any (owned:
    path == owned || lib.hasPrefix (owned + "/") path || lib.hasPrefix (path + "/") owned
  ) configuration.copy;
  inherit (import ./macos-settings.nix) nightShift;
in
{
  _file = toString ./home.nix;

  home.activation.configureNightShift = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    ${pkgs.nightlight}/bin/nightlight schedule ${nightShift.schedule.start} ${nightShift.schedule.end}
    ${pkgs.nightlight}/bin/nightlight temp ${toString nightShift.temperature}
  '';

  home.stateVersion = "26.05";
  home.packages = toolset.packages ++ [ toolset.dotfiles ] ++ artifacts.homePackages;

  assertions = [ {
    assertion = !configuration.localllm.enabled || (
      options.home.packages.highestPrio == 100 && builtins.elem artifacts.selected.localllm.package config.home.packages
    );
    message = "private home.packages definitions conflict with the public localllm owner";
  } {
    assertion = options.home.file.highestPrio == 100 && builtins.all (file:
      !file.enable || !(overlapsCopy file.target)
    ) (builtins.attrValues config.home.file);
    message = "home.file targets conflict with dotfiles copy paths";
  } ];

  home.file = lib.filterAttrs (path: _: !(overlapsCopy path)) artifacts.homeFiles;
}
