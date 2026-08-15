{ config, dotfilesDirectory, dotfilesSourceHome, ... }:

let
  primaryUser = config.system.primaryUser;
in
{
  imports = [ ./system.nix ];

  users.users.${primaryUser}.home = "/Users/${primaryUser}";

  home-manager = {
    useGlobalPkgs = true;
    backupFileExtension = "pre-home-manager";
    extraSpecialArgs = {
      inherit dotfilesDirectory dotfilesSourceHome;
    };
    users.${primaryUser} = import ./home.nix;
  };
}
