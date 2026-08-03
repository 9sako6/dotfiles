{ pkgs, primaryUser, ... }:

let
  homebrewPackages = import ./homebrew-packages.nix;
in
{
  assertions = [
    {
      assertion = primaryUser != "";
      message = "install:system must provide the primary macOS user";
    }
  ];

  environment.systemPackages = [
    pkgs.git
  ];

  homebrew = {
    enable = true;
    inherit (homebrewPackages) brews casks;
    global.autoUpdate = false;
    onActivation = {
      autoUpdate = false;
      cleanup = "uninstall";
      upgrade = false;
    };
    taps = [ ];
  };

  nix = {
    enable = true;
    package = pkgs.lix;
    settings.experimental-features = [
      "flakes"
      "nix-command"
    ];
  };

  nix-homebrew = {
    enable = true;
    autoMigrate = false;
    enableRosetta = false;
    enableZshIntegration = false;
    mutableTaps = false;
    user = primaryUser;
  };

  programs.zsh.shellInit = builtins.readFile ./homebrew-shellenv.zsh;

  services.zundamonotify.enable = true;

  system = {
    defaults = {
      NSGlobalDomain = {
        # 外観をダークモードにする
        AppleInterfaceStyle = "Dark";
        # 文頭の英字を自動で大文字にしない
        NSAutomaticCapitalizationEnabled = false;
        # ナチュラルスクロールを無効にする
        "com.apple.swipescrolldirection" = false;
      };

      # 右下のホットコーナーには何も割り当てない
      dock.wvous-br-corner = 1;

      finder = {
        # ゴミ箱に入れてから30日が過ぎた項目を自動で削除する
        FXRemoveOldTrashItems = true;
        # Finderの新しいウィンドウでホームフォルダを開く
        NewWindowTarget = "Home";
      };

      menuExtraClock = {
        # 12時間表示の時刻にAM/PMを付ける
        ShowAMPM = true;
        # メニューバーに空きがあるときは日付も表示する
        ShowDate = 0;
        # メニューバーの時計に曜日を表示する
        ShowDayOfWeek = true;
      };
    };
    keyboard = {
      # キーの割り当て変更を有効にする
      enableKeyMapping = true;
      # Caps LockをControlとして使う
      remapCapsLockToControl = true;
    };
    inherit primaryUser;
    stateVersion = 6;
  };
}
