# AGENTS.md

管理境界、Bootstrap の原則、バージョンの固定方法は [docs/repo-map.md](docs/repo-map.md) を正本とする。
変更前は必ず [docs/operations.md](docs/operations.md) の手順を確認する。

## ファイル管理

- 手書きで管理するリソース列挙はアルファベティカルに保つ

### バージョン

バージョン指定は次の基準に従う。

```mermaid
flowchart LR
    target{"指定する場所"}
    target -->|"mise [tools]"| mise["major.minor.patch"]
    target -->|"GitHub Actions"| actions["commit SHA とバージョンコメント"]
    target -->|"mise bootstrap の brew / brew-cask package"| brew["固定の例外"]
    target -->|"install.sh（バージョン管理ツールの導入前）"| permission{"ユーザーが緩和を明示したか"}
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
- `README.md` は編集しない
