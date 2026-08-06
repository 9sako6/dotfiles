{
  description = "Declarative macOS system configuration";

  inputs = {
    nix-darwin = {
      url = "github:nix-darwin/nix-darwin/c3e90c89649b07d1a96e4b9dd6cd0d6e44b91a74";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nix-homebrew.url = "github:zhaofengli/nix-homebrew/937ce52c7d046310571f3a070713804ead496843";
    nixpkgs.url = "github:NixOS/nixpkgs/705e9929918b43bd7b715dc0a878ac870449bb03";
    zundamonotify = {
      url = "github:9sako6/zundamonotify/fb44bf5226f3ab292f5b9fd8cf81bc94b454c2cf";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nix-darwin, nix-homebrew, zundamonotify, ... }:
    let
      system = "aarch64-darwin";
      primaryUser = builtins.getEnv "DARWIN_PRIMARY_USER";
      mkDarwinSystem = {
        configurationRevision ? null,
        modules ? [ ],
        primaryUser,
      }:
        nix-darwin.lib.darwinSystem {
          modules = [
            self.darwinModules.default
            {
              nixpkgs.hostPlatform = system;
              system = {
                inherit configurationRevision primaryUser;
              };
            }
          ] ++ modules;
        };
      publicSystem = mkDarwinSystem {
        inherit primaryUser;
        configurationRevision = self.rev or self.dirtyRev or null;
      };
    in
    {
      darwinConfigurations.current = publicSystem;

      darwinModules.default = {
        imports = [
          nix-homebrew.darwinModules.nix-homebrew
          zundamonotify.darwinModules.default
          ./configuration.nix
        ];
      };

      lib = {
        inherit mkDarwinSystem;
      };
    };
}
