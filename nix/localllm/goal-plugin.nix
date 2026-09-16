{ pkgs }:
let
  version = (builtins.fromJSON (builtins.readFile ./goal-plugin/package.json)).version;
  source = pkgs.fetchzip {
    url = "https://registry.npmjs.org/@prevalentware/opencode-goal-plugin/-/opencode-goal-plugin-${version}.tgz";
    hash = "sha256-KbLfJ3BSW+svU8R4d3ldj9tc1ZpmzWb2fKaJlQq1vIc=";
  };
in
pkgs.buildNpmPackage {
  pname = "localllm-goal-plugin";
  inherit version;
  src = ./goal-plugin;
  npmDepsHash = "sha256-TIAALNWDHyO5LWbFq9MnBG2cidXaQwbbmG0Tr5Bsng4=";
  nativeBuildInputs = [ pkgs.bun ];
  buildPhase = ''
    runHook preBuild
    cp ${source}/dist/server.js server.js
    bun build server.js --target bun --outfile dist/server.js
    runHook postBuild
  '';
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/dist" "$out/src"
    cp dist/server.js "$out/dist/server.js"
    cp ${source}/src/tui.ts "$out/src/tui.ts"
    cp ${source}/package.json ${source}/LICENSE "$out/"
    for dependency in effect fast-check pure-rand zod; do
      mkdir -p "$out/share/licenses/$dependency"
      cp node_modules/$dependency/LICENSE* "$out/share/licenses/$dependency/"
    done
    runHook postInstall
  '';
}
