# AGENTS.md

管理境界、Bootstrap の原則、バージョンの固定方法は [docs/repo-map.md](docs/repo-map.md) を正本とする。
日常の操作手順は [docs/operations.md](docs/operations.md) にまとめている。

## 変更前の確認

変更前は次の順に管理区分を確定する。

```mermaid
flowchart TD
    start["変更対象を確認"] --> known{"管理区分を判断できるか"}
    known -->|"はい"| boundary{"管理区分"}
    known -->|"いいえ"| map["docs/repo-map.md を確認"]
    map --> resolved{"管理区分を判断できたか"}
    resolved -->|"はい"| boundary
    resolved -->|"いいえ"| proposal["不足している判断基準と更新案を整理"]
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

## ファイル管理

- 手書きで管理するリソース列挙はアルファベティカルに保つ

### バージョン

バージョン指定は次の基準に従う。

```mermaid
flowchart LR
    target{"指定する場所"}
    target -->|"mise"| mise["major.minor.patch"]
    target -->|"GitHub Actions"| actions["commit SHA とバージョンコメント"]
    target -->|"Homebrew Brewfile"| brew["固定の例外"]
    target -->|"install.sh / devcontainer（バージョン管理ツールの導入前）"| permission{"ユーザーが緩和を明示したか"}
    target -->|"その他"| exact["厳密に固定する"]
    permission -->|"はい"| simple["簡潔さを優先し、固定を緩めてよい"]
    permission -->|"いいえ"| exact
```

`latest`、`^x.y`、`~x.y`、`@v4` などは固定されたバージョンとして扱わない。GitHub Actions の指定例は `@abc123 # v4.3.1`。

- `dist/` に秘密情報を入れない。秘密は `~/.zsh.d/secrets.zsh` に置く

## 検証

変更後は次の手順で契約テストを通す。

```mermaid
flowchart TD
    change["変更する"] --> testable{"振る舞いをテストできるか"}
    testable -->|"いいえ"| redesign["テストできる形へ直す"]
    redesign --> testable
    testable -->|"はい"| observe["コマンドやスクリプトを実行し、振る舞いを観測する"]
    observe --> test["mise run dev:test"]
    test --> passed{"成功したか"}
    passed -->|"いいえ"| change
    passed -->|"はい"| complete["完了"]
```

設定ファイルやソースの文面を直接検査するテストは書かない。

## Git と作業ファイル

- 作業ブランチは作らない。変更は `master` に直接コミットし、`master` を push する
- 一時的な作業メモは `tmp/` に置く（`tmp/plans/`, `tmp/specs/`）
- `README.md` は編集しない
