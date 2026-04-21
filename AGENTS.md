# AGENTS.md

設計判断（管理境界・Bootstrap 原則・バージョンピン留め）→ [docs/repo-map.md](docs/repo-map.md)
運用手順 → [docs/operations.md](docs/operations.md)

## ルール

- 変更前に、そのファイルが `repo runtime` と `dist-managed user tools` のどちらに属するかを判定する。迷ったら [docs/repo-map.md](docs/repo-map.md) を正本として確認する
- 自動生成ファイルの列挙順は生成器に委ね、手で整えない
- 手書きで管理するリソース列挙はアルファベティカルに保つ
- バージョンはすべて厳密にピン留めする。mise は `major.minor.patch`、GitHub Actions は commit SHA + バージョンコメントで固定（例: `@abc123 # v4.3.1`）（`latest`・`^x.y`・`~x.y`・`@v4` などの曖昧な指定は禁止。Homebrew Brewfile は例外）
- Bootstrap（`install.sh`）と devcontainer は、バージョン管理ツール導入前の環境なのでシンプルさを優先し、ユーザーの明示的な許可の上でピン留めを緩和できる
- `dist/` に秘密情報を入れない。秘密は `~/.zsh.d/secrets.zsh` に置く
- 変更後は `mise run dev:test` で契約テストを通す
- テストが通らない変更を完了にしない
- テストは設定ファイルやソースの文面を直接検査しない。コマンドやスクリプトを実行して振る舞いを観測する
- テストしにくければ、先にテスタブルな設計へ寄せる
- 作業ブランチは作らない。変更は `master` に直接コミットし、`master` を push する
- 一時的な作業メモは `tmp/` に置く（`tmp/plans/`, `tmp/specs/`）
- README.mdの編集は禁止
