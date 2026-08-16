{ pkgs }:

let
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
in
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
{
  packages = [
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
  ];
}
