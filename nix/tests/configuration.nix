{ lib, pkgs }:
let
  parse = public: local: import ../configuration.nix {
    inherit lib;
    publicFile = builtins.toFile "dotfiles.toml" public;
    localFile = if local == null then null else builtins.toFile "dotfiles.local.toml" local;
  };
  valid = public: local: (parse public local).errors == [ ];
  merged = (parse "copy = []" "[private]\npath = 'relative'").config;
  results = {
    copyAccepted = valid "copy = ['a', 'b']" null;
    privateLocalOnly = valid "copy = []" "[private]\npath = 'relative'";
    recursiveMerge = merged.private.path == "relative" && merged.copy == [ ];
    bad = builtins.all (pair: !(valid (builtins.elemAt pair 0) (builtins.elemAt pair 1))) [
      [ "copy = []" "copy = []" ]
      [ "[private]\npath = '../private'" null ]
      [ "copy = []" "[private]\npath = 2" ]
      [ "copy = []" "[_module]\nargs = {}" ]
      [ "copy = []" "unknown = 'secret-do-not-print'" ]
      [ "copy = ['../private']" null ]
      [ "copy = ['/private']" null ]
      [ "copy = ['a','a/b']" null ]
      [ "copy = ['b','a']" null ]
      [ "copy = ['a','a']" null ]
    ];
  };
in
assert lib.assertMsg (builtins.all (value: value) (builtins.attrValues results)) "configuration behavior test failed";
pkgs.writeText "configuration-tests-passed" (builtins.toJSON results)
