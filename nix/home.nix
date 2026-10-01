{ config, configuration, inputs, lib, options, pkgs, dotfilesDirectory, dotfilesSourceHome, ... }:

let
  ankiConnectAddon = "${toolset.ankiConnect}/share/anki/addons/anki-connect";
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
  home.packages = toolset.packages ++ [ toolset.dotfiles ] ++ lib.optional configuration.localllm.enabled (toolset.localllm configuration.localllm);

  assertions = [ {
    assertion = !configuration.localllm.enabled || (
      options.home.packages.highestPrio == 100 && builtins.elem (toolset.localllm configuration.localllm) config.home.packages
    );
    message = "private home.packages definitions conflict with the public localllm owner";
  } {
    assertion = options.home.file.highestPrio == 100 && builtins.all (file:
      !file.enable || !(overlapsCopy file.target)
    ) (builtins.attrValues config.home.file);
    message = "home.file targets conflict with dotfiles copy paths";
  } ];

  home.file = lib.filterAttrs (path: _: !(overlapsCopy path)) {
    "Library/Application Support/Anki2/addons21/anki-connect".source = ankiConnectAddon;
  };
}
