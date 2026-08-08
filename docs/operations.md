# 運用ガイド

macOS 前提。管理境界は [repo-map.md](repo-map.md) の「管理境界」を正本とする。

## 管理方針

- `repo runtime`: この repository 自身を動かすためのもの
- `home-managed user tools`: `home/` に置き、Home Manager を nix-darwin 経由で反映する
- `system configuration`: `darwin/` に置き、nix-darwin で反映する
- `private system configuration`: 公開できない差分だけを private root flake に置く
- `local-only`: repository に入れず各マシンへ置く
- `secrets`: repository と `home/` のどちらにも入れない

`apm.lock.yaml` などの生成物は手で編集せず、生成元のコマンドから更新する。

## 初回セットアップ

```sh
curl -fsSL https://dot.9sako6.com | sh
```

`install.sh` は mise を用意したあと `system:apply` を実行する。Home Manager は nix-darwin module として組み込まれているため、Mac 全体の設定と `home/` の配備は同じ system generation で反映される。

旧 home deployer から Home Manager へ初めて移行するとき、既存の通常ファイルと衝突した場合は `.pre-home-manager` suffix へ退避する。既存 symlink が予期した管理元を指していない場合は activation を止め、対象を確認してから再実行する。

## 日常コマンド

```sh
git pull                       # 公開dotfilesを通常のGit操作で更新
mise run doctor                # mise、system、Homebrewを診断
mise run system:apply          # system + Home Managerを確認して反映
mise run system:plan           # system + Home Managerをbuildしてplanを表示
mise run system:rollback       # 直前のnix-darwin世代へ戻す
mise run system:update         # public flake inputsを更新して検証
mise run test                  # 契約テストを実行
```

## Home Manager

`home/` の共有設定は `darwin/home.nix` から Home Manager の `home.file` へ宣言する。現段階では設定ファイルの中身を Nix 式へ移さず、`mkOutOfStoreSymlink` で live dotfiles checkout を参照する。

旧 deployer で symlink だった top-level path は live symlink として管理する。`.agents`、`.claude`、`.codex` は runtime file と共存できるよう recursive file deployment を使い、管理対象の leaf file だけを link する。

public system は `system:plan` / `system:apply` を実行している dotfiles checkout を自動で Home Manager へ渡す。`install.sh` の `DOTFILES_DIR` で checkout 先を変更した場合も同じ path を使う。private root flake が既定の `~/dotfiles` 以外を使う場合は `lib.mkDarwinSystem` の `dotfilesDirectory` 引数で明示する。

## system source

引数なしでは現在選択中の source を使う。未選択時は公開 dotfiles の local checkout が既定になる。
公開 source は自動で pull しない。private source は、最後に選択した credential を含まない SSH または HTTPS clone URL の remote default branch を取得し、push 済みの最新 commit を使う。

```sh
mise run system:plan <clone-url>   # 別sourceを試すが選択は変えない
mise run system:apply <clone-url>  # 成功後にsourceを選択する
mise run system:plan --default     # 公開sourceを試す
mise run system:apply --default    # 公開sourceへ戻す
```

`system:plan` は fetch、download、build、cache 更新を行うが、active system、Homebrew、source 選択を変更しない。`system:apply` は表示した同じ build 済み世代だけを activation する。Home Manager の activation もこの system activation に含まれる。

private repository は `darwin/flake.nix.template` を root の `flake.nix` としてコピーし、`primaryUser` を実際の macOS account name に置き換える。公開できない差分だけを `modules` に追加し、`flake.lock` と一緒に commit / push する。

公開側は `darwinModules.default` と `lib.mkDarwinSystem` を提供する。private root flake はこの module を利用するため、Home Manager の共有 home 設定も重複せず継承する。

## ロールバック

```sh
mise run system:rollback
```

remote の取得や flake の再評価をせず、保持済みの直前の世代へ戻す。Home Manager も system generation に含まれるため、home configuration もその世代に対応した状態へ戻る。

Nix のガベージコレクションは日本時間で毎週日曜日の 0:00 に実行し、14日を超えた世代を削除する。削除された世代へはロールバックできない。

## 検証

変更した振る舞いをコマンドやスクリプトで観測してから `mise run test` を実行する。

- `home-managed user tools` / `system configuration`: `mise run system:plan` → `mise run system:apply`
- `private system configuration`: private repository を push → `mise run system:plan` → `mise run system:apply`
- `repo runtime`: 反映コマンドなし

Homebrew 本体は nix-homebrew、formula と cask は nix-darwin、home directory の共有設定は Home Manager が管理する。
