{ config, lib, ... }:

let
  primaryUser = config.system.primaryUser;
in
{
  options.programs.dotfiles.repositoryDirectory = lib.mkOption {
    type = lib.types.str;
    default = "/Users/${primaryUser}/dotfiles";
    description = "Path to the live dotfiles repository checkout";
  };

  config = {
    users.users.${primaryUser}.home = "/Users/${primaryUser}";

    home-manager = {
      useGlobalPkgs = true;
      backupFileExtension = "pre-home-manager";
      extraSpecialArgs = {
        dotfilesDirectory = config.programs.dotfiles.repositoryDirectory;
      };
      users.${primaryUser} = import ./home.nix;
    };
  };
}
