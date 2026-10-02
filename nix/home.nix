# Compatibility defaults only. A private module must explicitly declare its
# Home Manager user; public files, packages and activation live outside HM.
{ config, configuration, dotfilesSourceHome, lib, options, ... }:

let
  files = prefix: directory:
    let entries = builtins.readDir directory;
    in lib.concatMap (name:
      if entries.${name} == "directory" then files "${prefix}/${name}" (directory + "/${name}")
      else [ "${prefix}/${name}" ]
    ) (builtins.attrNames entries);
  publicTargets = configuration.copy ++ [
    ".gitignore_global" ".local/bin/nightlight" ".zshenv" ".zshrc"
    "Library/Application Support/Anki2/addons21/anki-connect"
  ] ++ lib.optional configuration.localllm.enabled ".local/bin/localllm" ++ lib.concatMap (directory:
    files directory (dotfilesSourceHome + "/${directory}")
  ) [ ".config" ".zsh.d" "mybin" ];
  overlapsPublic = target:
    let path = if target == config.home.homeDirectory then ""
      else lib.removePrefix (config.home.homeDirectory + "/") target;
    in builtins.any (owned:
      path == "" || path == owned || lib.hasPrefix (owned + "/") path || lib.hasPrefix (path + "/") owned
    ) publicTargets;
in
{
  _file = toString ./home.nix;

  home.stateVersion = lib.mkDefault "26.05";

  assertions = [ {
    assertion = options.home.file.highestPrio == 100 && builtins.all (file:
      !file.enable || !(overlapsPublic file.target)
    ) (builtins.attrValues config.home.file);
    message = "home.file targets conflict with public dotfiles targets";
  } ];
}
