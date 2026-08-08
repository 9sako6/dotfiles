# 運用ガイド

macOS 前提。管理境界は [repo-map.md](repo-map.md) の「管理境界」を正本とする。

## 変更前の確認

変更前は、管理区分を次の順で確定する。

```mermaid
flowchart TD
    start["変更対象を確認"] --> known{"管理区分を判断できるか"}
    known -->|"はい"| boundary{"管理区分"}
    known -->|"いいえ"| map["docs/repo-map.md を確認"]
    map --> resolved{"管理区分を判断できたか"}
    resolved -->|"はい"| boundary
    resolved -->|"いいえ"| proposal["不足している判断基準を特定し、更新案を作る"]
    proposal --> ask["ユーザーに確認する"]
    boundary -->|"repo runtime"| repo["リポジトリ固有のルールは project rule に置く"]
    boundary -->|"home-managed user tools"| home["skill には配備先でも使える一般ルールだけを書く"]
    boundary -->|"system configuration"| system["Mac 全体の設定は darwin/ に置く"]
    boundary -->|"private system configuration"| private["公開できない差分だけを別の root flake に置く"]
    boundary -->|"local-only"| local["repo に入れず、各マシンに置く"]
    boundary -->|"secrets"| secrets["repo と home/ に入れず、最終判断をユーザーに確認する"]
    repo --> change["変更に進む"]
    home --> change
    system --> change
    private --> change
```

### 生成物

`apm.lock.yaml` などの生成物は手で編集しない。

```mermaid
flowchart TD
    target["変更対象"] --> generated{"生成物か"}
    generated -->|"いいえ"| edit["直接変更する"]
    generated -->|"はい"| locate["生成元と再生成手順を探す"]
    locate --> found{"手順を特定できたか"}
    found -->|"いいえ"| ask["ユーザーに確認する"]
    found -->|"はい"| regenerate["正規の手順で再生成する"]
    regenerate --> order["列挙順も生成器に委ねる"]
```

## 初回セットアップ

```sh
curl -fsSL https://dot.9sako6.com | sh
```

Home Manager は nix-darwin module として組み込まれているため、system と home は同じ `system:apply` で反映する。
旧 home deployer から初めて移行するとき、Home Manager が管理する既存ファイルとの衝突は `.pre-home-manager` suffix へ退避される。
`curl | sh` では確認入力だけを制御端末から読み、download 中の script を回答として消費しない。

## 日常コマンド

```sh
git pull                       # 公開dotfilesを通常のGit操作で更新
mise run doctor                # mise、system、Homebrewを診断
mise run system:apply          # system + Home Managerを確認して反映
mise run system:plan           # system + Home Managerをbuildしてplanを表示
mise run system:rollback       # 直前のnix-darwin世代へ戻す
mise run test                  # 契約テストを実行
```

他の task は `mise tasks` で一覧できる。

Home Manager は `home/` の設定を live dotfiles checkout への out-of-store link として管理する。
`.agents`、`.claude`、`.codex` は tracked tree を Nix 側で列挙して各 leaf file だけを link するため、ディレクトリ自体は runtime file と共存できる。
public system は `system:plan` / `system:apply` を実行している checkout を自動で使う。private root flake が既定の `~/dotfiles` 以外を使う場合は、`lib.mkDarwinSystem` の `dotfilesDirectory` 引数で明示する。

## system source

引数なしでは現在選択中の source を使う。未選択時は公開 dotfiles の local checkout が既定になる。
公開 source は自動で pull しない。private source は、最後に選択した credential を含まない SSH または
HTTPS clone URL の remote default branch を取得し、push 済みの最新 commit を使う。

```sh
mise run system:plan <clone-url>   # 別sourceを試すが選択は変えない
mise run system:apply <clone-url>  # 成功後にsourceを選択する
mise run system:plan --default     # 公開sourceを試す
mise run system:apply --default    # 公開sourceへ戻す
```

`system:plan` は fetch、download、build、cache 更新を行うが、active system、Homebrew、source 選択を
変更しない。Lix がなければ失敗する。`system:apply` は必要なら Lix を導入し、表示した同じ build 済み
世代だけを activation する。Home Manager の activation もこの system activation に含まれる。
plan には system closure の差分と Homebrew cleanup 候補が現れる。
fetch、認証、flake 評価に失敗した場合、古い cache へ fallback しない。`system:apply` は activation と
source 選択を一度の `sudo` 実行で完了し、長い activation の後に認証を再要求しない。同じ source
selection を使う `system:apply` が実行中なら、後から開始した処理を拒否する。

private repository は `darwin/flake.nix.template` を root の `flake.nix` としてコピーし、
`primaryUser` を実際の macOS account name に置き換える。公開できない差分だけを `modules` に追加し、
`nix flake lock` で生成した `flake.lock` と一緒に commit する。

```nix
modules = [
  {
    homebrew.casks = [
      "private-app"
    ];
  }
];
```

公開側は `darwinModules.default` と `lib.mkDarwinSystem` を提供する。public source だけが実行時の
macOS account name と live dotfiles checkout を受け取るため、private root flake では `primaryUser` を明示する。
source の選択状態は `/etc/nix-darwin/flake.nix` の symlink だけであり、未知の既存ファイルや symlink は
明示引数があっても置換しない。

## ロールバック

`mise run system:rollback` は remote の取得や flake の評価をせず、保持済みの直前の世代へ戻す。
system source の選択は変えないため、次の `system:plan` は同じ source を診断する。

Nix のガベージコレクションは日本時間で毎週日曜日の 0:00 に実行し、14日を超えた世代を削除する。
削除された世代へはロールバックできない。手動で `nix-collect-garbage` を実行する場合も、
削除対象に必要な世代が含まれないことを確認してから実行する。

## 検証

変更した振る舞いをコマンドやスクリプトで観測してから、`mise run test` を実行する。振る舞いをテストできない場合は、観測可能な境界を作ってから変更する。

設定ファイルやソースの文面を直接検査するテストは書かない。

## 変更前後の基本手順

1. 上の手順で管理区分を確定
2. `home-managed user tools` を変更する場合は、`mise run system:plan` で配備を含む system generation を確認
3. 必要な変更を入れる
4. 「検証」の手順を実施
5. 変更した管理区分に応じて反映
   - `home-managed user tools` — `mise run system:apply`
   - `system configuration` — `mise run system:apply`
   - `private system configuration` — private repository を push して `mise run system:apply`

`repo runtime` の変更に反映コマンドはない。`system:apply` の初回実行では、
Lix を導入するため途中で `sudo` の認証を求められる。

Homebrew 本体は nix-homebrew、formula と cask は nix-darwin、home directory の共有設定は Home Manager が管理する。

GitHub-hosted macOS runner には管理外の Homebrew が導入済みのため、CI は system derivation の
build までを検証し、activation は行わない。新規 Mac への activation の E2E は、Homebrew のない
VM または実機で確認する。公開インストールの smoke test も user environment の導入と system
derivation の build を分けて検証する。
