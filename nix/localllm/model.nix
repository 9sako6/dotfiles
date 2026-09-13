{ pkgs, model }:
pkgs.linkFarm "localllm-model-${model.revision}" (map (file: {
  inherit (file) name;
  path = pkgs.fetchurl {
    url = "https://huggingface.co/${model.repository}/resolve/${model.revision}/${file.name}";
    inherit (file) hash;
  };
}) model.files)
