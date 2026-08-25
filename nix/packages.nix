{ pkgs }:

let
  ankiConnectPackage = pkgs.ankiAddons.anki-connect;
  expectedAnkiConnectVersion = "25.11.9.0";

  ankiPackage = pkgs.anki-bin;
  expectedAnkiVersion = "26.05";

  awscliPackage = pkgs.awscli2;
  expectedAwscliVersion = "2.35.11";

  bunPackage = pkgs.bun;
  expectedBunVersion = "1.3.13";

  gitPackage = pkgs.git;
  expectedGitVersion = "2.55.0";

  goPackage = pkgs.go_1_26;
  expectedGoVersion = "1.26.5";

  nightlightPackage = pkgs.nightlight;
  expectedNightlightVersion = "1.0.0";

  quintPackage = pkgs.quint;
  expectedQuintVersion = "0.32.0";

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
assert pkgs.lib.assertMsg (awscliPackage.version == expectedAwscliVersion)
  "AWS CLI version drifted: expected ${expectedAwscliVersion}, got ${awscliPackage.version}";
assert pkgs.lib.assertMsg (bunPackage.version == expectedBunVersion)
  "Bun version drifted: expected ${expectedBunVersion}, got ${bunPackage.version}";
assert pkgs.lib.assertMsg (gitPackage.version == expectedGitVersion)
  "Git version drifted: expected ${expectedGitVersion}, got ${gitPackage.version}";
assert pkgs.lib.assertMsg (goPackage.version == expectedGoVersion)
  "Go version drifted: expected ${expectedGoVersion}, got ${goPackage.version}";
assert pkgs.lib.assertMsg (nightlightPackage.version == expectedNightlightVersion)
  "Nightlight version drifted: expected ${expectedNightlightVersion}, got ${nightlightPackage.version}";
assert pkgs.lib.assertMsg (quintPackage.version == expectedQuintVersion)
  "Quint version drifted: expected ${expectedQuintVersion}, got ${quintPackage.version}";
assert pkgs.lib.assertMsg (rustToolchain.rustc.version == expectedRustVersion)
  "Rust version drifted: expected ${expectedRustVersion}, got ${rustToolchain.rustc.version}";
assert pkgs.lib.assertMsg (terminalBrowserPackage.version == expectedTerminalBrowserVersion)
  "terminal-browser version drifted: expected ${expectedTerminalBrowserVersion}, got ${terminalBrowserPackage.version}";
{
  ankiConnect = ankiConnectPackage;

  packages = [
    # Anki 26.05
    ankiPackage

    # AWS CLI 2.35.11
    awscliPackage

    # Bun 1.3.13
    bunPackage

    # Git 2.55.0
    gitPackage

    # Go 1.26.5
    goPackage

    # Nightlight 1.0.0
    nightlightPackage

    # Quint 0.32.0
    quintPackage

    # Rust 1.97.1
    rustToolchain.rustc
    rustToolchain.cargo
    rustToolchain.rustfmt
    rustToolchain.clippy

    # terminal-browser 0.6.0
    terminalBrowserPackage
  ];
}
