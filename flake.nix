{
  description = "Declarative macOS system and home configuration";

  inputs = {
    home-manager = {
      url = "github:nix-community/home-manager/master";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    homebrew-tinycast = {
      url = "github:abue-ammar/homebrew-tinycast/main";
      flake = false;
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

  outputs = { self, home-manager, nix-darwin, nix-homebrew, zundamonotify, nixpkgs, ... }@inputs:
    let
      system = "aarch64-darwin";
      pkgs = nixpkgs.legacyPackages.${system};
      toolset = import ./nix/packages.nix { inherit pkgs; };
      artifacts = import ./nix/artifacts.nix { inherit pkgs toolset; };
      primaryUser = let user = builtins.getEnv "DARWIN_PRIMARY_USER"; in if user == "" then "fixture" else user;
      defaultConfiguration = (import ./nix/configuration.nix {
        inherit (nixpkgs) lib;
        publicFile = ./dotfiles.toml;
      }).config;
      dotfilesSourceHome = self.outPath + "/home";
      publicDotfilesDirectory =
        let
          configured = builtins.getEnv "DOTFILES_DIR";
        in
        if configured == "" then "/Users/${primaryUser}/dotfiles" else configured;
      mkHost = {
        configuration ? defaultConfiguration,
        configurationRevision ? null,
        dotfilesDirectory,
        primaryUser,
        privateFlake ? null,
      }:
        assert nixpkgs.lib.assertMsg (privateFlake == null || privateFlake ? darwinModules.default)
          "private.path must export darwinModules.default";
        nix-darwin.lib.darwinSystem {
          specialArgs = {
            inherit configuration dotfilesDirectory dotfilesSourceHome inputs;
          };
          modules = [
            self.darwinModules.default
            ({ pkgs, ... }: {
              environment.systemPackages = [ pkgs.anki-bin ];
              assertions = [ {
                assertion = pkgs.anki-bin.version == "26.05";
                message = "Anki version drifted: expected 26.05, got ${pkgs.anki-bin.version}";
              } {
                assertion = toString pkgs.path == nixpkgs.outPath;
                message = "private modules must use the public nixpkgs package set";
              } ];
            })
            {
              nixpkgs.hostPlatform = system;
              system = {
                inherit configurationRevision primaryUser;
              };
            }
          ] ++ nixpkgs.lib.optional (privateFlake != null) privateFlake.darwinModules.default;
        };
      publicSystem = mkHost {
        inherit primaryUser;
        configurationRevision = self.rev or self.dirtyRev or null;
        dotfilesDirectory = publicDotfilesDirectory;
      };
    in
    {
      packages.${system} = {
        inherit artifacts;
        default = artifacts;
      };

      checks.${system} = {
        artifacts = import ./nix/tests/artifacts.nix { inherit pkgs; };
        composition = import ./nix/tests/composition.nix { inherit self pkgs; inherit (nixpkgs) lib; };
        configuration = import ./nix/tests/configuration.nix { inherit (nixpkgs) lib; inherit pkgs; };
      };

      darwinConfigurations.current = publicSystem;

      darwinModules.default = {
        imports = [
          home-manager.darwinModules.home-manager
          nix-homebrew.darwinModules.nix-homebrew
          zundamonotify.darwinModules.default
          ./nix
        ];
      };

      lib = {
        inherit mkHost;
      };
    };
}
