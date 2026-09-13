{ inputs, pkgs }:
let
  workspace = inputs.uv2nix.lib.workspace.loadWorkspace { workspaceRoot = ./.; };
  python = pkgs.python313;
  base = pkgs.callPackage inputs.pyproject-nix.build.packages { inherit python; };
  packages = base.overrideScope (pkgs.lib.composeManyExtensions [
    inputs.pyproject-build-systems.overlays.wheel
    (workspace.mkPyprojectOverlay { sourcePreference = "wheel"; })
    (final: previous: {
      mlx = previous.mlx.overrideAttrs (old: {
        nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ pkgs.cctools ];
        postFixup = (old.postFixup or "") + ''
          install_name_tool -change @rpath/libmlx.dylib \
            ${final.mlx-metal}/${python.sitePackages}/mlx/lib/libmlx.dylib \
            "$out/${python.sitePackages}/mlx/core"*.so
        '';
      });
    })
  ]);
in
{
  environment = packages.mkVirtualEnv "dotfiles-localllm-runtime" workspace.deps.default;
}
