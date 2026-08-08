{ config, dotfilesDirectory, ... }:

let
  primaryUser = config.system.primaryUser;
in
{
  users.users.${primaryUser}.home = "/Users/${primaryUser}";

  home-manager = {
    useGlobalPkgs = true;
    backupFileExtension = "pre-home-manager";
    extraSpecialArgs = {
      inherit dotfilesDirectory;
    };
    users.${primaryUser} = import ./home.nix;
  };
}
