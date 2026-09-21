{ configuration, host, inputs, publicSource, privateSource ? null }:
let
  inherit (host.pkgs) lib;
  cfg = host.config;
  user = cfg.system.primaryUser;
  home = cfg.home-manager.users.${user};
  toolset = import ./packages.nix { pkgs = host.pkgs; inherit inputs; };
  nixPackage = package: {
    name = lib.getName package;
    declared = lib.getVersion package;
    manager = "Nix";
  };
  miseFile = path:
    let
      config = builtins.fromTOML (builtins.readFile (publicSource + "/${path}"));
      version = value:
        if builtins.isString value then value
        else if builtins.isList value then lib.concatMapStringsSep ", " version value
        else value.version or "unspecified";
    in lib.mapAttrsToList (name: value: {
      inherit name;
      declared = version value;
      manager = "mise";
    }) (config.tools or { });
  brewPackage = entry: {
    name = entry.name;
    manager = "Homebrew (nix-darwin)";
    declared = "—";
  };
  flatten = path: value:
    if builtins.isAttrs value then lib.concatMap (key: flatten (path ++ [ key ]) value.${key}) (builtins.attrNames value)
    else lib.optional (value != null) {
      key = lib.concatStringsSep "." path;
      group = lib.concatStringsSep "." (lib.init path);
      name = lib.last path;
      inherit value;
    };
  settings = path: options: values:
    lib.concatMap (name:
      let
        option = options.${name};
        value = values.${name} or null;
        owned = definition: lib.any (source: lib.hasPrefix (toString source + "/") definition.file)
          ([ publicSource ] ++ lib.optional (privateSource != null) privateSource);
      in
      if lib.isOption option then
        lib.optionals (option.isDefined && builtins.any owned option.definitionsWithLocations && value != null)
          (flatten (path ++ [ name ]) (option.type.merge (path ++ [ name ]) option.definitionsWithLocations))
      else if builtins.isAttrs option && builtins.isAttrs value then settings (path ++ [ name ]) option value
      else [ ]) (builtins.attrNames options);
  withoutNulls = value:
    if builtins.isAttrs value then lib.mapAttrs (_: withoutNulls) (lib.filterAttrs (_: v: v != null) value)
    else if builtins.isList value then map withoutNulls value
    else value;
  service = scope: name: job: {
    inherit name scope;
    config = withoutNulls (builtins.intersectAttrs {
      KeepAlive = null;
      RunAtLoad = null;
      StartCalendarInterval = null;
      StartInterval = null;
      WatchPaths = null;
      QueueDirectories = null;
      StartOnMount = null;
      Sockets = null;
    } job.serviceConfig);
  };
  services = scope: jobs:
    lib.mapAttrsToList (service scope) (lib.filterAttrs (_: job: job.enable or true) jobs);
  agentPath = path: lib.any (prefix: lib.hasPrefix prefix path)
    [ ".agents/" ".claude/" ".codex/" ".config/opencode/" ];
in {
  schemaVersion = 3;
  source = publicSource;
  packages = map nixPackage (builtins.filter (p: lib.getName p != "localllm" && lib.getVersion p != "")
    (home.home.packages ++ cfg.environment.systemPackages))
    ++ [ (nixPackage toolset.ankiConnect) ]
    ++ miseFile "home/.config/mise/config.toml"
    ++ miseFile ".mise.toml"
    ++ map brewPackage (cfg.homebrew.brews ++ cfg.homebrew.casks);
  system = settings [ "system" "defaults" ] host.options.system.defaults cfg.system.defaults
    ++ settings [ "system" "keyboard" ] host.options.system.keyboard cfg.system.keyboard
    ++ flatten [ "time" "timeZone" ] cfg.time.timeZone
    ++ flatten [ ] (import ./macos-settings.nix)
    ++ flatten [ ] {
      homebrew = {
        global = { inherit (cfg.homebrew.global) autoUpdate; };
        onActivation = { inherit (cfg.homebrew.onActivation) autoUpdate cleanup upgrade; };
      };
      nix.gc = { inherit (cfg.nix.gc) automatic options; };
      nix-homebrew = { inherit (cfg.nix-homebrew) mutableTaps; };
    };
  services = services "system" cfg.launchd.daemons
    ++ services "all users" cfg.launchd.agents
    ++ services "user" cfg.launchd.user.agents
    ++ services "user" home.launchd.agents;
  timeZone = cfg.time.timeZone;
  tools = map (path: { inherit path; deploy = "copy"; }) (builtins.filter agentPath configuration.copy)
    ++ lib.mapAttrsToList (_: file: { path = file.target; deploy = "symlink"; })
      (lib.filterAttrs (_: file: file.enable && agentPath file.target) home.home.file);
  localllm = builtins.intersectAttrs { enabled = null; default_model = null; } configuration.localllm;
}
