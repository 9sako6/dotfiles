{ config, configuration, inputs, lib, options, pkgs, dotfilesDirectory, dotfilesSourceHome, ... }:

let
  ankiConnectAddon = "${toolset.ankiConnect}/share/anki/addons/anki-connect";
  homeRoot = "${dotfilesDirectory}/home";
  toolset = import ./packages.nix { inherit inputs pkgs; };
  outOfStore = relativePath:
    config.lib.file.mkOutOfStoreSymlink "${homeRoot}/${relativePath}";
  liveLink = relativePath: {
    source = outOfStore relativePath;
  };
  overlapsCopy = path: builtins.any (owned:
    path == owned || lib.hasPrefix (owned + "/") path || lib.hasPrefix (path + "/") owned
  ) configuration.copy;
  collectLiveFiles = relativeRoot: sourceRoot:
    let
      entries = if builtins.any (owned: relativeRoot == owned || lib.hasPrefix (owned + "/") relativeRoot) configuration.copy
        then { } else builtins.readDir sourceRoot;
    in
    builtins.foldl'
      (files: name:
        let
          entryType = entries.${name};
          relativePath = "${relativeRoot}/${name}";
          sourcePath = sourceRoot + "/${name}";
        in
        files // (
          if entryType == "directory" then
            collectLiveFiles relativePath sourcePath
          else
            { ${relativePath} = liveLink relativePath; }
        ))
      { }
      (builtins.attrNames entries);
  liveFiles = builtins.foldl'
    (files: relativeRoot:
      files // collectLiveFiles relativeRoot (dotfilesSourceHome + "/${relativeRoot}"))
    { }
    [
      ".config"
      ".zsh.d"
      "mybin"
    ];
  inherit (import ./macos-settings.nix) nightShift;
in
{
  _file = toString ./home.nix;

  home.activation.configureNightShift = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    ${pkgs.nightlight}/bin/nightlight schedule ${nightShift.schedule.start} ${nightShift.schedule.end}
    ${pkgs.nightlight}/bin/nightlight temp ${toString nightShift.temperature}
  '';

  home.stateVersion = "26.05";
  home.packages = toolset.packages
    ++ lib.optional configuration.codex_go.enabled toolset.codexGo
    ++ lib.optional configuration.localllm.enabled (toolset.localllm configuration.localllm);

  launchd.agents.codex-go = lib.mkIf configuration.codex_go.enabled {
    enable = true;
    config = {
      ProgramArguments = [ "${toolset.codexGo}/bin/codex-go-proxy" ];
      RunAtLoad = true;
      KeepAlive.SuccessfulExit = false;
      ThrottleInterval = 30;
      StandardErrorPath = "${config.home.homeDirectory}/Library/Logs/codex-go.log";
      StandardOutPath = "${config.home.homeDirectory}/Library/Logs/codex-go.log";
    };
  };

  assertions = [ {
    assertion = !configuration.localllm.enabled || (
      options.home.packages.highestPrio == 100 && builtins.elem (toolset.localllm configuration.localllm) config.home.packages
    );
    message = "private home.packages definitions conflict with the public localllm owner";
  } {
    assertion = options.home.file.highestPrio == 100 && builtins.all (file:
      !file.enable || !(overlapsCopy file.target)
    ) (builtins.attrValues config.home.file);
    message = "home.file targets conflict with dotfiles copy paths";
  } ];

  home.file = lib.filterAttrs (path: _: !(overlapsCopy path)) ({
    ".gitconfig" = liveLink ".gitconfig";
    ".gitignore_global" = liveLink ".gitignore_global";
    ".zshenv" = liveLink ".zshenv";
    ".zshrc" = liveLink ".zshrc";
    "Library/Application Support/Anki2/addons21/anki-connect".source = ankiConnectAddon;
    "apm.lock.yaml" = liveLink "apm.lock.yaml";
    "apm.yml" = liveLink "apm.yml";
  } // liveFiles // lib.optionalAttrs configuration.codex_go.enabled {
    ".codex/opencode-go.config.toml".source = (pkgs.formats.toml { }).generate "opencode-go.config.toml" {
      model = "deepseek-v4.1-flash";
      model_auto_compact_token_limit = 100000;
      model_catalog_json = "${toolset.codexGo}/share/codex-go/models.json";
      model_provider = "opencode-go";
      model_reasoning_effort = "none";
      model_reasoning_summary = "none";
      model_verbosity = "low";
      service_tier = "default";
      web_search = "disabled";
      model_providers.opencode-go = {
        name = "OpenCode Go (LiteLLM)";
        base_url = "http://127.0.0.1:4010/v1";
        wire_api = "responses";
        auth.command = "${toolset.codexGo}/bin/codex-go-auth";
      };
    };
  });
}
