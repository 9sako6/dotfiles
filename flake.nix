{
  description = "Declarative macOS system and home configuration";

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
    pyproject-build-systems = {
      url = "github:pyproject-nix/build-system-pkgs/master";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.pyproject-nix.follows = "pyproject-nix";
      inputs.uv2nix.follows = "uv2nix";
    };
    pyproject-nix = {
      url = "github:pyproject-nix/pyproject.nix/master";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    uv2nix = {
      url = "github:pyproject-nix/uv2nix/master";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.pyproject-nix.follows = "pyproject-nix";
    };
    zundamonotify = {
      url = "github:9sako6/zundamonotify";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, home-manager, nix-darwin, nix-homebrew, zundamonotify, nixpkgs, ... }@inputs:
    let
      system = "aarch64-darwin";
      pkgs = nixpkgs.legacyPackages.${system};
      toolset = import ./nix/packages.nix { inherit inputs pkgs; };
      userToolsPackage = pkgs.buildEnv {
        name = "dotfiles-user-tools";
        paths = toolset.packages;
      };
      dotfilesPackage = pkgs.rustPlatform.buildRustPackage {
        pname = "dotfiles";
        version = "0.1.0";
        src = ./cli;
        cargoLock.lockFile = ./cli/Cargo.lock;
      };
      primaryUser = let user = builtins.getEnv "DARWIN_PRIMARY_USER"; in if user == "" then "fixture" else user;
      defaultConfiguration = (import ./nix/configuration.nix {
        inherit (nixpkgs) lib;
        publicFile = ./dotfiles.toml;
      }).config;
      dotfilesSourceHome = self.outPath + "/home";
      mkDarwinSystem = {
        configurationRevision ? null,
        dotfilesDirectory ? "/Users/${primaryUser}/dotfiles",
        modules ? [ ],
        configuration ? defaultConfiguration,
        primaryUser,
      }:
        let
          darwinSystem = nix-darwin.lib.darwinSystem {
            specialArgs = {
              inherit configuration dotfilesDirectory dotfilesSourceHome inputs;
            };
            modules = [
              self.darwinModules.default
              ({ pkgs, ... }: {
                environment.systemPackages = [ dotfilesPackage ];
                assertions = [ {
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
      mkHost = {
        configuration ? defaultConfiguration,
        configurationRevision ? null,
        dotfilesDirectory,
        primaryUser,
        privateFlake ? null,
      }:
        assert nixpkgs.lib.assertMsg (privateFlake == null || privateFlake ? darwinModules.default)
          "private.path must export darwinModules.default";
        mkDarwinSystem {
          inherit configuration configurationRevision dotfilesDirectory primaryUser;
          modules = nixpkgs.lib.optional (privateFlake != null) privateFlake.darwinModules.default;
        };
      publicSystem = mkHost {
        inherit primaryUser;
        configurationRevision = self.rev or self.dirtyRev or null;
        dotfilesDirectory = publicDotfilesDirectory;
      };
    in
    {
      packages.${system} = {
        default = dotfilesPackage;
        dotfiles = dotfilesPackage;
        localllm = toolset.localllm (defaultConfiguration.localllm // {
          default_model = "qwen3.8-27b-4bit";
          enabled = true;
          models = [ "qwen3.8-27b-4bit" ];
        });
        localllmClient = toolset.localllmClient;
        localllmRuntime = toolset.localllmRuntime;
        userTools = userToolsPackage;
      };

      checks.${system} = {
        composition = import ./nix/tests/composition.nix { inherit self pkgs; inherit (nixpkgs) lib; };
        configuration = import ./nix/tests/configuration.nix { inherit (nixpkgs) lib; inherit pkgs; };
        modelFetch = pkgs.fetchurl {
          url = "https://huggingface.co/mlx-community/Qwen3.8-27B-4bit/resolve/3e6447f082e89cc7f0bc6e5441afd38dfce760ff/generation_config.json";
          hash = "sha256-5wwTbBt43cH7CQW6yOczpNxEjU+FKl3XUUP//HC+VQ4=";
        };
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
