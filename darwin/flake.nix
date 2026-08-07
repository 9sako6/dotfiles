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
      url = "github:9sako6/zundamonotify/47910c54e5eaf2f7c5464f7c36686f1870e5db16";
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
        let
          darwinSystem = nix-darwin.lib.darwinSystem {
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
        in
        darwinSystem // {
          homebrewBrewfile = darwinSystem.pkgs.writeText
            "Brewfile"
            darwinSystem.config.homebrew.brewfile;
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
