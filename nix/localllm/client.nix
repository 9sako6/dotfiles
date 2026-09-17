{ pkgs }:
assert pkgs.lib.assertMsg (pkgs.opencode.version == "1.18.13") "OpenCode version drifted from 1.18.13";
if pkgs.stdenv.hostPlatform.isDarwin then
  pkgs.opencode.overrideAttrs (old: {
    nativeBuildInputs = old.nativeBuildInputs ++ [ pkgs.cctools pkgs.darwin.sigtool ];
    postPatch = old.postPatch + ''
      substituteInPlace packages/opencode/script/build.ts \
        --replace-fail \
          'if (item.os === process.platform && item.arch === process.arch && !item.abi)' \
          'if (false)'
    '';
    postInstall = ''
      codesign --force --sign - "$out/bin/.opencode-wrapped"
    '' + old.postInstall;
    dontStrip = true;
  })
else
  pkgs.opencode
