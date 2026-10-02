{ directory, user }:
let
  public = builtins.getFlake ("git+file://" + directory);
  local = directory + "/dotfiles.local.toml";
  configuration = (import ./configuration.nix {
    inherit (public.inputs.nixpkgs) lib;
    publicFile = public.outPath + "/dotfiles.toml";
    localFile = if builtins.pathExists local then local else null;
  }).config;
  privatePath = configuration.private.path;
  privateDirectory = if privatePath == null then null
    else if public.inputs.nixpkgs.lib.hasPrefix "/" privatePath then privatePath
    else directory + "/" + privatePath;
  host = public.lib.mkHost {
    inherit configuration;
    dotfilesDirectory = directory;
    primaryUser = user;
    privateFlake = if privateDirectory == null then null
      else builtins.getFlake ("git+file://" + privateDirectory);
    configurationRevision = public.rev or public.dirtyRev or null;
  };
  system = host.system;
  resources = (public.lib.mkArtifacts { inherit configuration; }).root;
in
{
  inherit system resources;
  bundle = public.inputs.nixpkgs.legacyPackages.aarch64-darwin.linkFarm "dotfiles-system" [
    { name = "resources"; path = resources; }
    { name = "system"; path = system; }
  ];
}
