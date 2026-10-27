final: prev:
{
  brotli = prev.brotli.overrideAttrs {
    version = "1.2.1";
    src = final.fetchFromGitHub {
      owner = "google";
      repo = "brotli";
      tag = "v1.2.1";
      hash = "sha256-llY72cYffDv0ugsH0p89xIPEbElV4L8u7U/Fe8hT3gI=";
    };
    patches = [ ];
  };
}
