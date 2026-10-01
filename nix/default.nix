{ config, configuration, dotfilesDirectory, dotfilesSourceHome, inputs, lib, options, ... }:

let
  primaryUser = config.system.primaryUser;
in
{
  imports = [ ./system.nix ];

  options.dotfiles.configuration = lib.mkOption {
    type = lib.types.raw;
    readOnly = true;
  };

  config = {
    dotfiles.configuration = configuration;
    assertions = [ {
      assertion = options.dotfiles.configuration.highestPrio == 100;
      message = "private module conflicts with dotfiles configuration ownership";
    } ];

    users.users.${primaryUser}.home = "/Users/${primaryUser}";

    home-manager = {
      useGlobalPkgs = true;
      backupFileExtension = "pre-home-manager";
      extraSpecialArgs = {
        inherit configuration dotfilesDirectory dotfilesSourceHome inputs;
      };
      # A private module may declare any user. Public configuration does not
      # create a Home Manager user or own a user activation package.
      sharedModules = [ ./home.nix ];
    };
  };
}
