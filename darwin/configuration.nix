{ pkgs, primaryUser, ... }:

{
  assertions = [
    {
      assertion = primaryUser != "";
      message = "install:system must provide the primary macOS user";
    }
  ];

  nix = {
    enable = true;
    package = pkgs.lix;
    settings.experimental-features = [
      "flakes"
      "nix-command"
    ];
  };

  system = {
    inherit primaryUser;
    stateVersion = 6;
  };
}
