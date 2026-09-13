let
  manifest = builtins.getEnv "DOTFILES_INPUT_MANIFEST";
  operation = builtins.getEnv "DOTFILES_INPUT_OPERATION";
  input = builtins.fromJSON (builtins.readFile manifest);
  public = builtins.getFlake ("path:" + input.publicSource);
  parsed = import ./configuration.nix {
    inherit (public.inputs.nixpkgs) lib;
    publicFile = input.publicSource + "/dotfiles.toml";
    localFile = input.localFile;
  };
  private = if input.privateSource == null then null
    else builtins.getFlake ("path:" + input.privateSource);
  host = public.lib.mkHost {
    configuration = parsed.config;
    dotfilesDirectory = input.directory;
    primaryUser = input.user;
    privateFlake = private;
    configurationRevision = input.publicRevision;
  };
in
if operation == "configuration" || operation == "settings" then {
  inherit (parsed) errors;
  config = if parsed.errors != [ ] then null
    else if operation == "settings" then parsed.settings
    else parsed.config;
}
else if operation == "outputs" then {
  brewfile = host.homebrewBrewfile.drvPath;
  copy = parsed.config.copy;
  localllm = parsed.config.localllm;
  system = host.system.drvPath;
}
else throw "unknown host input operation"
