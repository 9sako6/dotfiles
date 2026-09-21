{ lib, pkgs }:
let
  parse = public: local: import ../configuration.nix {
    inherit lib;
    publicFile = builtins.toFile "dotfiles.toml" public;
    localFile = if local == null then null else builtins.toFile "dotfiles.local.toml" local;
  };
  valid = public: local: (parse public local).errors == [ ];
  enabled = ''
    [localllm]
    enabled = true
    models = ["qwen3.8-27b-4bit"]
    default_model = "qwen3.8-27b-4bit"
  '';
  disabled = ''
    [localllm]
    enabled = false
    models = []
  '';
  merged = (parse enabled ''
    [localllm]
    enabled = false
    models = []
  '').config;
  results = {
    defaults = (parse "copy = {}" null).config.localllm == { enabled = false; models = [ ]; default_model = null; };
    sharedSource = (parse "copy = { a = 'AGENTS.md', b = 'AGENTS.md' }" null).config.copy
      == { a = "AGENTS.md"; b = "AGENTS.md"; };
    enable = valid "copy = {}" enabled;
    falseValue = (parse enabled "[localllm]\nenabled = false").config.localllm.enabled == false;
    arrayReplacement = (parse "[localllm]\nmodels = ['qwen3.8-27b-4bit']" disabled).config.localllm.models == [ ];
    falseEmptyTogether = merged.localllm.enabled == false && merged.localllm.models == [ ];
    recursiveTable = (parse enabled "[localllm]\nenabled = false").config.localllm.default_model == "qwen3.8-27b-4bit";
    bad = builtins.all (pair: !(valid (builtins.elemAt pair 0) (builtins.elemAt pair 1))) [
      [ "copy = {}" "copy = {}" ]
      [ "[private]\npath = '../private'" null ]
      [ "copy = {}" "[private]\npath = 2" ]
      [ "copy = {}" "[_module]\nargs = {}" ]
      [ "copy = {}" "[localllm._module]\nargs = {}" ]
      [ "copy = {}" "unknown = 'secret-do-not-print'" ]
      [ "copy = {}" "[localllm]\nenabld = true" ]
      [ "copy = {}" "[localllm]\nenabled = 2" ]
      [ "copy = {}" "[localllm]\nmodels = [2]" ]
      [ "copy = {}" "[localllm]\nenabled = true" ]
      [ "copy = {}" "[localllm]\nenabled = true\nmodels = ['qwen3.8-27b-4bit']" ]
      [ "copy = {}" "[localllm]\nmodels = ['unknown']" ]
      [ "copy = {}" "[localllm]\nmodels = ['qwen3.8-27b-4bit','qwen3.8-27b-4bit']" ]
      [ "copy = {}" "[localllm]\nmodels = ['z','a']" ]
      [ "copy = []" null ]
      [ "copy = { a = 2 }" null ]
      [ "copy = { a = {} }" null ]
      [ "copy = { '../private' = 'source' }" null ]
      [ "copy = { '/private' = 'source' }" null ]
      [ "copy = { a = '../private' }" null ]
      [ "copy = { a = '/private' }" null ]
      [ "copy = { a = '' }" null ]
      [ "copy = { a = 'source', 'a/b' = 'source' }" null ]
    ];
    noValueDisclosure = builtins.all (message: !(lib.hasInfix "secret-do-not-print" message)) (parse "copy = {}" "unknown = 'secret-do-not-print'").errors;
  };
in
assert lib.assertMsg (builtins.all (value: value) (builtins.attrValues results)) "configuration behavior test failed";
pkgs.writeText "configuration-tests-passed" (builtins.toJSON results)
