{ pkgs, inputs }:

let
  ankiConnectPackage = pkgs.ankiAddons.anki-connect;
  expectedAnkiConnectVersion = "25.11.9.0";

  ankiPackage = pkgs.anki-bin;
  expectedAnkiVersion = "26.05";

  antigravityCliPackage = (import pkgs.path {
    inherit (pkgs.stdenv.hostPlatform) system;
    config.allowUnfreePredicate = package: pkgs.lib.getName package == "antigravity-cli";
  }).antigravity-cli.overrideAttrs (previous: {
    version = "1.2.2";
    src = pkgs.fetchurl {
      url = "https://storage.googleapis.com/antigravity-public/antigravity-cli/1.2.2-6061403484848128/darwin-arm/cli_mac_arm64.tar.gz";
      hash = "sha512-ijte3qUeEHp0QTzqXu0cWwLe3pRbohx2JbfIb0d8LIrEI1gd4O2E/y+Xw79oGMH8JMqEz5p2wvsCpQ6J2KDSmg==";
    };
    nativeBuildInputs = (previous.nativeBuildInputs or [ ]) ++ [ pkgs.makeWrapper ];
    postFixup = (previous.postFixup or "") + ''
      wrapProgram "$out/bin/agy" --set AGY_CLI_DISABLE_AUTO_UPDATE true
    '';
    meta = previous.meta // {
      platforms = [ "aarch64-darwin" ];
    };
  });
  expectedAntigravityCliVersion = "1.2.2";

  awscliPackage = pkgs.awscli2;
  expectedAwscliVersion = "2.35.11";

  bunPackage = pkgs.bun;
  expectedBunVersion = "1.3.13";

  fdPackage = pkgs.fd.overrideAttrs (final: previous: {
    version = "10.5.0";
    src = pkgs.fetchFromGitHub {
      owner = "sharkdp";
      repo = "fd";
      rev = "v${final.version}";
      hash = "sha256-X3yrZUBieMO5bauUu2mskfvicLRdQcjbXouFTpz+0eQ=";
    };
    cargoHash = "sha256-5ijtWSsHn7ii8+1njwyvyj387FQkMQALKbixkr/nSkk=";
    cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
      inherit (final) pname src version;
      hash = final.cargoHash;
    };
  });
  expectedFdVersion = "10.5.0";

  ffmpegPackage = pkgs.ffmpeg;
  expectedFfmpegVersion = "8.1.2";

  fzfPackage = pkgs.fzf.overrideAttrs (final: previous: {
    version = "0.74.4";
    src = pkgs.fetchFromGitHub {
      owner = "junegunn";
      repo = "fzf";
      tag = "v${final.version}";
      hash = "sha256-QQ4hwbTeZ6MSojjCFn7dlISz1aGQbewGq4wnfs3/4K0=";
    };
  });
  expectedFzfVersion = "0.74.4";

  ghqPackage = pkgs.ghq;
  expectedGhqVersion = "1.10.1";

  gitPackage = pkgs.git;
  expectedGitVersion = "2.55.0";

  goPackage = pkgs.go_1_26;
  expectedGoVersion = "1.26.5";

  herdrPackage = pkgs.stdenvNoCC.mkDerivation {
    pname = "herdr";
    version = "0.8.2";
    src = pkgs.fetchurl {
      url = "https://github.com/herdrdev/herdr/releases/download/v0.8.2/herdr-macos-aarch64";
      hash = "sha256-pdT01QTYswnJH4EQUFWTAPq6MSWEJfU8UIUvyW9q5XQ=";
    };
    dontUnpack = true;
    installPhase = ''
      runHook preInstall
      install -Dm755 "$src" "$out/bin/herdr"
      runHook postInstall
    '';
    meta = {
      description = "Runtime for persistent coding-agent workspaces";
      homepage = "https://github.com/herdrdev/herdr";
      license = pkgs.lib.licenses.asl20;
      mainProgram = "herdr";
      platforms = [ "aarch64-darwin" ];
    };
  };
  expectedHerdrVersion = "0.8.2";

  nightlightPackage = pkgs.nightlight;
  expectedNightlightVersion = "1.0.0";

  quintPackage = pkgs.quint;
  expectedQuintVersion = "0.32.0";

  ripgrepPackage = pkgs.ripgrep;
  expectedRipgrepVersion = "15.2.0";

  rustToolchain = pkgs.rustPackages_1_97;
  expectedRustVersion = "1.97.1";

  terminalBrowserPackage = pkgs.stdenvNoCC.mkDerivation {
    pname = "terminal-browser";
    version = "0.6.0";
    src = pkgs.fetchurl {
      url = "https://github.com/zenbu-labs/terminal-browser/releases/download/v0.6.0/terminal-browser-darwin-arm64.tar.gz";
      hash = "sha256-0tGgYLYgjxyMUEoa+CXu0PsFv629iyPx4AZWGcV350k=";
    };
    sourceRoot = "terminal-browser";
    dontFixup = true;
    installPhase = ''
      runHook preInstall
      mkdir -p "$out"
      cp -R . "$out/"
      runHook postInstall
    '';
    meta = {
      description = "Real browser that runs inside a terminal";
      homepage = "https://github.com/zenbu-labs/terminal-browser";
      license = pkgs.lib.licenses.mit;
      mainProgram = "terminal-browser";
      platforms = [ "aarch64-darwin" ];
    };
  };
  expectedTerminalBrowserVersion = "0.6.0";
in
assert pkgs.lib.assertMsg (ankiConnectPackage.version == expectedAnkiConnectVersion)
  "AnkiConnect version drifted: expected ${expectedAnkiConnectVersion}, got ${ankiConnectPackage.version}";
assert pkgs.lib.assertMsg (ankiPackage.version == expectedAnkiVersion)
  "Anki version drifted: expected ${expectedAnkiVersion}, got ${ankiPackage.version}";
assert pkgs.lib.assertMsg (antigravityCliPackage.version == expectedAntigravityCliVersion)
  "Antigravity CLI version drifted: expected ${expectedAntigravityCliVersion}, got ${antigravityCliPackage.version}";
assert pkgs.lib.assertMsg (awscliPackage.version == expectedAwscliVersion)
  "AWS CLI version drifted: expected ${expectedAwscliVersion}, got ${awscliPackage.version}";
assert pkgs.lib.assertMsg (bunPackage.version == expectedBunVersion)
  "Bun version drifted: expected ${expectedBunVersion}, got ${bunPackage.version}";
assert pkgs.lib.assertMsg (fdPackage.version == expectedFdVersion)
  "fd version drifted: expected ${expectedFdVersion}, got ${fdPackage.version}";
assert pkgs.lib.assertMsg (ffmpegPackage.version == expectedFfmpegVersion)
  "FFmpeg version drifted: expected ${expectedFfmpegVersion}, got ${ffmpegPackage.version}";
assert pkgs.lib.assertMsg (fzfPackage.version == expectedFzfVersion)
  "fzf version drifted: expected ${expectedFzfVersion}, got ${fzfPackage.version}";
assert pkgs.lib.assertMsg (ghqPackage.version == expectedGhqVersion)
  "ghq version drifted: expected ${expectedGhqVersion}, got ${ghqPackage.version}";
assert pkgs.lib.assertMsg (gitPackage.version == expectedGitVersion)
  "Git version drifted: expected ${expectedGitVersion}, got ${gitPackage.version}";
assert pkgs.lib.assertMsg (goPackage.version == expectedGoVersion)
  "Go version drifted: expected ${expectedGoVersion}, got ${goPackage.version}";
assert pkgs.lib.assertMsg (herdrPackage.version == expectedHerdrVersion)
  "Herdr version drifted: expected ${expectedHerdrVersion}, got ${herdrPackage.version}";
assert pkgs.lib.assertMsg (nightlightPackage.version == expectedNightlightVersion)
  "Nightlight version drifted: expected ${expectedNightlightVersion}, got ${nightlightPackage.version}";
assert pkgs.lib.assertMsg (quintPackage.version == expectedQuintVersion)
  "Quint version drifted: expected ${expectedQuintVersion}, got ${quintPackage.version}";
assert pkgs.lib.assertMsg (ripgrepPackage.version == expectedRipgrepVersion)
  "ripgrep version drifted: expected ${expectedRipgrepVersion}, got ${ripgrepPackage.version}";
assert pkgs.lib.assertMsg (rustToolchain.rustc.version == expectedRustVersion)
  "Rust version drifted: expected ${expectedRustVersion}, got ${rustToolchain.rustc.version}";
assert pkgs.lib.assertMsg (terminalBrowserPackage.version == expectedTerminalBrowserVersion)
  "terminal-browser version drifted: expected ${expectedTerminalBrowserVersion}, got ${terminalBrowserPackage.version}";
{
  ankiConnect = ankiConnectPackage;
  localllmClient = assert pkgs.opencode.version == "1.18.13"; pkgs.opencode;
  localllmGoalPlugin = import ./localllm/goal-plugin.nix { inherit pkgs; };
  localllmRuntime = (import ./localllm/runtime.nix { inherit inputs pkgs; }).environment;
  localllm = configuration: import ./localllm/package.nix { inherit configuration inputs pkgs; };

  packages = [
    # Anki 26.05
    ankiPackage

    antigravityCliPackage

    # AWS CLI 2.35.11
    awscliPackage

    # Bun 1.3.13
    bunPackage

    fdPackage

    ffmpegPackage

    fzfPackage

    ghqPackage

    # Git 2.55.0
    gitPackage

    # Go 1.26.5
    goPackage

    # Herdr 0.8.2
    herdrPackage

    # Nightlight 1.0.0
    nightlightPackage

    # Quint 0.32.0
    quintPackage

    ripgrepPackage

    # Rust 1.97.1
    rustToolchain.rustc
    rustToolchain.cargo
    rustToolchain.rustfmt
    rustToolchain.clippy

    # terminal-browser 0.6.0
    terminalBrowserPackage
  ];
}
