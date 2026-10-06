{ lib, publicFile, localFile ? null }:
let
  inherit (lib) mkOption types;
  schema = {
    options = {
      copy = mkOption { type = types.listOf types.str; default = [ ]; };
      private = mkOption {
        default = { };
        type = types.submodule {
          options.path = mkOption { type = types.nullOr types.str; default = null; };
        };
      };
    };
  };
  options = (lib.evalModules { modules = [ schema ]; }).options;
  validType = type: value: type.check value && (
    if builtins.isList value && type ? nestedTypes.elemType then
      builtins.all (validType type.nestedTypes.elemType) value
    else true
  );
  check = file: prefix: opts: value:
    lib.concatMap (key:
      let
        name = prefix + key;
        option = opts.${key} or null;
        children = if option == null then { } else option.type.getSubOptions [ ];
      in
      if key == "_module" || option == null then [ "${file}: ${name}: unknown key" ]
      else if !(validType option.type value.${key}) then [ "${file}: ${name}: invalid type" ]
      else if children != { } && builtins.isAttrs value.${key} then
        check file (name + ".") children value.${key}
      else [ ]) (builtins.attrNames value);
  systemConfiguration = value: builtins.removeAttrs value [ "services" "settings" ];
  public = systemConfiguration (builtins.fromTOML (builtins.readFile publicFile));
  local = if localFile == null then { } else systemConfiguration (builtins.fromTOML (builtins.readFile localFile));
  structuralErrors = check "dotfiles.toml" "" options public
    ++ check "dotfiles.local.toml" "" options local
    ++ lib.optional (public ? private) "dotfiles.toml: private: only allowed in dotfiles.local.toml"
    ++ lib.optional (local ? copy) "dotfiles.local.toml: copy: only allowed in dotfiles.toml";
  merged = (lib.evalModules {
    modules = [ schema { config = lib.recursiveUpdate public local; } ];
  }).config;
  sortedUnique = values: values == lib.sort builtins.lessThan (lib.unique values);
  validPath = value: value != "" && !(lib.hasPrefix "/" value)
    && builtins.all (part: part != "" && part != "." && part != "..") (lib.splitString "/" value);
  valueErrors =
    lib.optional (!sortedUnique merged.copy) "dotfiles.toml: copy: entries must be unique and alphabetical"
    ++ lib.optional (!(builtins.all validPath merged.copy)) "dotfiles.toml: copy: invalid relative path"
    ++ lib.optional (builtins.any (a: builtins.any (b: a != b && lib.hasPrefix (a + "/") b) merged.copy) merged.copy)
      "dotfiles.toml: copy: entries must not overlap"
    ++ lib.optional (merged.private.path == "") "dotfiles.local.toml: private.path: must not be empty";
  errors = structuralErrors ++ lib.optionals (structuralErrors == [ ]) valueErrors;
  invalidConfiguration = throw ("dotfiles configuration is invalid:\n"
    + lib.concatMapStringsSep "\n" (error: "  " + error) errors);
  settings = path: value:
    if builtins.isAttrs value then
      lib.concatMap (key: settings (path ++ [ key ]) value.${key}) (builtins.attrNames value)
    else [ {
      key = lib.concatStringsSep "." path;
      inherit value;
      source = if lib.hasAttrByPath path local then "dotfiles.local.toml"
        else if lib.hasAttrByPath path public then "dotfiles.toml"
        else null;
    } ];
in
{
  inherit errors;
  config = if errors == [ ] then merged else invalidConfiguration;
  settings = if errors == [ ] then settings [ ] merged else invalidConfiguration;
}
