{ config, configuration, dotfilesDirectory, dotfilesSourceHome, inputs, ... }:

let
  primaryUser = config.system.primaryUser;
in
{
  imports = [ ./system.nix ];

  config = {
    users.users.${primaryUser}.home = "/Users/${primaryUser}";

    home-manager = {
      useGlobalPkgs = true;
      extraSpecialArgs = {
        inherit configuration dotfilesDirectory dotfilesSourceHome inputs;
      };
      # A private module may declare any user. Public configuration does not
      # create a Home Manager user or own a user activation package.
      sharedModules = [ ./home.nix ];
    };
  };
}
