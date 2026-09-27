{ codex, litellm, pkgs }:
let
  auth = pkgs.writeShellApplication {
    name = "codex-go-auth";
    runtimeInputs = [ pkgs.jq ];
    text = ''
      auth_file="''${XDG_DATA_HOME:-$HOME/.local/share}/opencode/auth.json"
      if ! jq -er '."opencode-go" | select(.type == "api") | .key | select(type == "string" and length > 0)' "$auth_file" 2>/dev/null; then
        echo 'OpenCode Go API key is missing. Connect OpenCode Go in OpenCode first.' >&2
        exit 1
      fi
    '';
  };
  proxyConfig = pkgs.writeText "codex-go.yaml" (builtins.toJSON {
    model_list = [ {
      model_name = "deepseek-v4.1-flash";
      litellm_params = {
        model = "deepseek/deepseek-v4.1-flash";
        api_base = "https://opencode.ai/zen/go/v1";
        api_key = "os.environ/OPENCODE_GO_API_KEY";
        extra_headers.User-Agent = "codex-go/1.0";
      };
      model_info.mode = "chat";
    } ];
    litellm_settings = {
      drop_params = true;
      telemetry = false;
    };
    general_settings = {
      forward_client_headers_to_llm_api = true;
      master_key = "os.environ/OPENCODE_GO_API_KEY";
    };
  });
  proxy = pkgs.writeShellApplication {
    name = "codex-go-proxy";
    text = ''
      OPENCODE_GO_API_KEY="$(${auth}/bin/codex-go-auth)"
      export OPENCODE_GO_API_KEY
      export LITELLM_LOCAL_MODEL_COST_MAP=True
      export LITELLM_LOG=ERROR
      export SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt
      exec ${litellm}/bin/litellm --config ${proxyConfig} --host 127.0.0.1 --port 4010 --num_workers 1 "$@"
    '';
  };
  client = pkgs.writeShellApplication {
    name = "codex-go";
    text = ''
      exec ${codex}/bin/codex --profile opencode-go "$@"
    '';
  };
in
pkgs.symlinkJoin {
  name = "codex-go-${litellm.version}";
  pname = "codex-go";
  inherit (litellm) version;
  paths = [ auth client proxy ];
  postBuild = ''
    mkdir -p "$out/share/codex-go"
    ln -s ${./models.json} "$out/share/codex-go/models.json"
    ln -s ${proxyConfig} "$out/share/codex-go/proxy.yaml"
  '';
  passthru = { inherit litellm; };
  meta.mainProgram = "codex-go";
}
