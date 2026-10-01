# Compatibility defaults only. A private module must explicitly declare its
# Home Manager user; public files, packages and activation live outside HM.
{ config, configuration, lib, options, ... }:

let
  overlapsCopy = target:
    let path = if target == config.home.homeDirectory then ""
      else lib.removePrefix (config.home.homeDirectory + "/") target;
    in builtins.any (owned:
      path == "" || path == owned || lib.hasPrefix (owned + "/") path || lib.hasPrefix (path + "/") owned
    ) configuration.copy;
in
{
  _file = toString ./home.nix;

  home.stateVersion = lib.mkDefault "26.05";

  assertions = [ {
    assertion = options.home.file.highestPrio == 100 && builtins.all (file:
      !file.enable || !(overlapsCopy file.target)
    ) (builtins.attrValues config.home.file);
    message = "home.file targets conflict with dotfiles copy paths";
  } ];
}
