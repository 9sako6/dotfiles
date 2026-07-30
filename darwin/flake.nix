{
  description = "Declarative macOS system configuration";

  inputs = {
    nix-darwin = {
      url = "github:nix-darwin/nix-darwin/56c666e108467d87d13508936aade6d567f2a501";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/705e9929918b43bd7b715dc0a878ac870449bb03";
  };

  outputs = { self, nix-darwin, ... }:
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
          ./configuration.nix
          {
            nixpkgs.hostPlatform = system;
            system.configurationRevision = self.rev or self.dirtyRev or null;
          }
        ];
      };
    };
}
