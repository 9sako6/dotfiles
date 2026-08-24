{ config, pkgs, ... }:

let
  homebrewPackages = import ./homebrew-packages.nix;
  primaryUser = config.system.primaryUser;
  screenshotDirectory = "/Users/${primaryUser}/screenshots";
in
{
  assertions = [
    {
      assertion = primaryUser != "";
      message = "system.primaryUser must name the primary macOS user";
    }
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
    gc = {
      automatic = true;
      interval = {
        Hour = 0;
        Minute = 0;
      };
      options = "--delete-older-than 2d";
    };
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

  # Fixed SSH agent socket so nix-daemon (root) can reuse the user's SSH keys
  # for git+ssh flake inputs (e.g. private hermes). The socket itself is
  # created on demand by home/.zsh.d/ssh-agent.zsh.
  launchd.daemons.nix-daemon.serviceConfig.EnvironmentVariables.SSH_AUTH_SOCK =
    "/Users/${primaryUser}/.ssh/agent.sock";

  services.zundamonotify.enable = true;

  system = {
    activationScripts = {
      # スクリーンショット保存先のディレクトリを用意する
      extraActivation.text = ''
        install -d -o "${primaryUser}" -g "$(id -gn "${primaryUser}")" "${screenshotDirectory}"
      '';
    };

    defaults = {
      NSGlobalDomain = {
        # 外観をダークモードにする
        AppleInterfaceStyle = "Dark";
        # 文頭の英字を自動で大文字にしない
        NSAutomaticCapitalizationEnabled = false;
        # ナチュラルスクロールを無効にする
        "com.apple.swipescrolldirection" = false;
      };

      controlcenter.Sound = true;

      # 右下のホットコーナーには何も割り当てない
      dock.wvous-br-corner = 1;

      # Dockの「最近使った項目」を非表示にする
      dock.show-recents = false;

      # 音声入力を有効にする
      CustomUserPreferences."com.apple.assistant.support"."Dictation Enabled" = true;
      hitoolbox.AppleFnUsageType = "Start Dictation";

      finder = {
        # ファイル拡張子を常に表示する
        AppleShowAllExtensions = true;
        # 隠しファイルも表示する
        AppleShowAllFiles = true;
        # Finderに「終了」メニューを追加する
        QuitMenuItem = true;
        # パスバーを表示する
        ShowPathbar = true;
        # ステータスバーを表示する
        ShowStatusBar = true;
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

      # スクリーンショットの保存先
      screencapture.location = screenshotDirectory;
    };
    keyboard = {
      # キーの割り当て変更を有効にする
      enableKeyMapping = true;
      # Caps LockをControlとして使う
      remapCapsLockToControl = true;
    };
    stateVersion = 6;
  };

  time.timeZone = "Asia/Tokyo";
}
