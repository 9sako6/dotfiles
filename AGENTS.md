# AGENTS.md

構成と安全境界 → [docs/repo-map.md](docs/repo-map.md)
運用コマンドと手順 → [docs/operations.md](docs/operations.md)

## ルール

- バージョンはすべて厳密にピン留めする。mise は `major.minor.patch`、package.json は範囲指定なし、GitHub Actions は commit SHA + バージョンコメントで固定（例: `@abc123 # v4.3.1`）（`latest`・`^x.y`・`~x.y`・`@v4` などの曖昧な指定は禁止。Homebrew Brewfile は例外）
- `dist/` に秘密情報を入れない。秘密は `~/.zsh.d/secrets.zsh` に置く
- 変更後は `mise run test` で契約テストを通す
- テストが通らない変更を完了にしない
- テストは設定ファイルやソースの文面を直接検査しない。コマンドやスクリプトを実行して振る舞いを観測する
- テストしにくければ、先にテスタブルな設計へ寄せる
- 作業ブランチは作らない。変更は `master` に直接コミットし、`master` を push する
- 一時的な作業メモは `tmp/` に置く（`tmp/plans/`, `tmp/specs/`）
