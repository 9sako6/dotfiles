{
  outputs = { self }:
    let
      input = builtins.fromJSON (builtins.readFile ./inputs.json);
      public = builtins.getFlake input.publicFlake;
      configuration = (import (public.outPath + "/nix/configuration.nix") {
        inherit (public.inputs.nixpkgs) lib;
        publicFile = public.outPath + "/dotfiles.toml";
        localFile = if input.localFile == null then null else ./. + "/${input.localFile}";
      }).config;
      host = public.lib.mkHost {
        inherit configuration;
        configurationRevision = input.publicRevision;
        dotfilesDirectory = input.directory;
        primaryUser = input.user;
        privateFlake = if input.privateFlake == null then null
          else builtins.getFlake input.privateFlake;
      };
    in {
      brewfile = host.homebrewBrewfile;
      system = host.system;
    };
}
