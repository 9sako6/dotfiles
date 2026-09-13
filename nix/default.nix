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
      users.${primaryUser} = import ./home.nix;
    };
  };
}
