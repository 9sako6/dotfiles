{ pkgs, ... }:

let
  gitPackage = pkgs.git;
  expectedGitVersion = "2.55.0";

  goPackage = pkgs.go_1_26;
  expectedGoVersion = "1.26.5";

  quintPackage = pkgs.quint;
  expectedQuintVersion = "0.32.0";

  rustToolchain = pkgs.rustPackages_1_97;
  expectedRustVersion = "1.97.1";
in
{
  assertions = [
    {
      assertion = gitPackage.version == expectedGitVersion;
      message = "Git version drifted: expected ${expectedGitVersion}, got ${gitPackage.version}";
    }
    {
      assertion = goPackage.version == expectedGoVersion;
      message = "Go version drifted: expected ${expectedGoVersion}, got ${goPackage.version}";
    }
    {
      assertion = quintPackage.version == expectedQuintVersion;
      message = "Quint version drifted: expected ${expectedQuintVersion}, got ${quintPackage.version}";
    }
    {
      assertion = rustToolchain.rustc.version == expectedRustVersion;
      message = "Rust version drifted: expected ${expectedRustVersion}, got ${rustToolchain.rustc.version}";
    }
  ];

  home.packages = [
    # Git 2.55.0
    gitPackage

    # Go 1.26.5
    goPackage

    # Quint 0.32.0
    quintPackage

    # Rust 1.97.1
    rustToolchain.rustc
    rustToolchain.cargo
    rustToolchain.rustfmt
    rustToolchain.clippy
  ];
}
