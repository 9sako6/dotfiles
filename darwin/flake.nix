{
  description = "Declarative macOS system configuration";

  inputs = {
    nix-darwin = {
      url = "github:nix-darwin/nix-darwin/56c666e108467d87d13508936aade6d567f2a501";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nix-homebrew.url = "github:zhaofengli/nix-homebrew/937ce52c7d046310571f3a070713804ead496843";
    nixpkgs.url = "github:NixOS/nixpkgs/705e9929918b43bd7b715dc0a878ac870449bb03";
    zundamonotify = {
      url = "github:9sako6/zundamonotify/09a246f93b2f2aa6307c49136751c0386fb5c485";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nix-darwin, nix-homebrew, zundamonotify, ... }:
    let
      system = "aarch64-darwin";
      primaryUser = builtins.getEnv "DARWIN_PRIMARY_USER";
    in
    {
      apps.${system}.darwin-rebuild = {
        type = "app";
        program = "${nix-darwin.packages.${system}.darwin-rebuild}/bin/darwin-rebuild";
      };

      darwinConfigurations.${system} = nix-darwin.lib.darwinSystem {
        specialArgs = {
          inherit primaryUser;
        };
        modules = [
          nix-homebrew.darwinModules.nix-homebrew
          zundamonotify.darwinModules.default
          ./configuration.nix
          {
            nixpkgs.hostPlatform = system;
            system.configurationRevision = self.rev or self.dirtyRev or null;
          }
        ];
      };
    };
}
