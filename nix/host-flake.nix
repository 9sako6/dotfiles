{
  outputs = { self }:
    let
      input = builtins.fromJSON (builtins.readFile ./inputs.json);
      public = builtins.getFlake input.publicFlake;
      # Rust passes the same validated, frozen configuration used by the Plan.
      # The fallback preserves direct consumers of the previous host interface.
      configuration = input.configuration or (import (public.outPath + "/nix/configuration.nix") {
        inherit (public.inputs.nixpkgs) lib;
        publicFile = public.outPath + "/dotfiles.toml";
        localFile = if input.localFile == null then null else ./. + "/${input.localFile}";
      }).config;
      host = public.lib.mkHost {
        inherit configuration;
        resourceSource = (builtins.getFlake (input.resourceFlake or input.publicFlake)).outPath;
        systemInputs = input.systemInputs or null;
        configurationRevision = input.publicRevision;
        dotfilesDirectory = input.directory;
        primaryUser = input.user;
        privateFlake = if input.privateFlake == null then null
          else builtins.getFlake input.privateFlake;
      };
    in {
      brewfile = host.homebrewBrewfile;
      inventory = host.inventory;
      system = host.system;
    };
}
