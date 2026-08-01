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
    boundary -->|"dist-managed user tools"| dist["skill には配布先でも使える一般ルールだけを書く"]
    boundary -->|"local-only"| local["repo に入れず、各マシンに置く"]
    boundary -->|"secrets"| secrets["repo と dist/ に入れず、最終判断をユーザーに確認する"]
    repo --> change["変更に進む"]
    dist --> change
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
curl -fsSL dot.9sako6.com | bash
```

既存ファイルは `~/.dotfiles-backups/` に退避される。

## 日常コマンド

```sh
mise run plan       # 配備計画を表示（filesystem は変更しない）
mise run apply      # dist/ を home directory に反映
mise run dev:test   # 契約テストを実行
```

他の task は `mise tasks` で一覧できる。

## 変更前後の基本手順

1. `mise run plan` で確認
2. 必要な変更を入れる
3. `mise run dev:test` で回帰を確認
4. `mise run apply` で反映
