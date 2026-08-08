{
  description = "Declarative macOS system configuration";

  inputs = {
    home-manager = {
      url = "github:nix-community/home-manager/master";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nix-darwin = {
      url = "github:nix-darwin/nix-darwin/master";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nix-homebrew.url = "github:zhaofengli/nix-homebrew";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    zundamonotify = {
      url = "github:9sako6/zundamonotify";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, home-manager, nix-darwin, nix-homebrew, zundamonotify, ... }:
    let
      system = "aarch64-darwin";
      primaryUser = builtins.getEnv "DARWIN_PRIMARY_USER";
      dotfilesSourceHome = self.sourceInfo.outPath + "/home";
      mkDarwinSystem = {
        configurationRevision ? null,
        dotfilesDirectory ? "/Users/${primaryUser}/dotfiles",
        modules ? [ ],
        primaryUser,
      }:
        let
          darwinSystem = nix-darwin.lib.darwinSystem {
            specialArgs = {
              inherit dotfilesDirectory dotfilesSourceHome;
            };
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
      publicDotfilesDirectory =
        let
          configured = builtins.getEnv "DOTFILES_DIR";
        in
        if configured == "" then "/Users/${primaryUser}/dotfiles" else configured;
      publicSystem = mkDarwinSystem {
        inherit primaryUser;
        configurationRevision = self.rev or self.dirtyRev or null;
        dotfilesDirectory = publicDotfilesDirectory;
      };
    in
    {
      darwinConfigurations.current = publicSystem;

      darwinModules.default = {
        imports = [
          home-manager.darwinModules.home-manager
          nix-homebrew.darwinModules.nix-homebrew
          zundamonotify.darwinModules.default
          ./home-manager.nix
          ./configuration.nix
        ];
      };

      lib = {
        inherit mkDarwinSystem;
      };
    };
}
