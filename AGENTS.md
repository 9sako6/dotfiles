# AGENTS.md

構成と安全境界 → [docs/repo-map.md](docs/repo-map.md)
運用コマンドと手順 → [docs/operations.md](docs/operations.md)

## ルール

- `dist/` に秘密情報を入れない。秘密は `~/.zsh.d/secrets.zsh` に置く
- 変更後は `mise run test` で契約テストを通す
- テストが通らない変更を完了にしない
- テストは設定ファイルやソースの文面を直接検査しない。コマンドやスクリプトを実行して振る舞いを観測する
- テストしにくければ、先にテスタブルな設計へ寄せる
- 一時的な作業メモは `tmp/` に置く（`tmp/plans/`, `tmp/specs/`）
