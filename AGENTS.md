# AGENTS.md

構成と安全境界 → [docs/repo-map.md](docs/repo-map.md)
運用コマンドと手順 → [docs/operations.md](docs/operations.md)

## ルール

- repo ルートと `dist/` は別物として扱う。repo ルートはこの repo 自体を開発・検証するための `repo runtime`、`dist/` は home directory に配備する `dist-managed user tools`
- 変更前に、そのファイルが `repo runtime` と `dist-managed user tools` のどちらに属するかを先に判定する。片側の整理や削除を理由に、もう片側のファイルまで連鎖的に消さない
- 具体例:
  - `.claude/`, `.codex/`, `scripts/`, `tests/`, `.mise.toml` は repo runtime。`dist/` の整理だけを理由に消さない
  - `dist/.claude/`, `dist/.codex/`, `dist/mybin/`, `dist/.zshrc` は配布物。home directory に配るのをやめるならこちらを触る
- 迷ったら「このファイルは home directory に配備される前提か？」で判断する。Yes なら `dist/` 側、No なら repo runtime 側
- バージョンはすべて厳密にピン留めする。mise は `major.minor.patch`、GitHub Actions は commit SHA + バージョンコメントで固定（例: `@abc123 # v4.3.1`）（`latest`・`^x.y`・`~x.y`・`@v4` などの曖昧な指定は禁止。Homebrew Brewfile は例外）
- `dist/` に秘密情報を入れない。秘密は `~/.zsh.d/secrets.zsh` に置く
- 変更後は `mise run test` で契約テストを通す
- テストが通らない変更を完了にしない
- テストは設定ファイルやソースの文面を直接検査しない。コマンドやスクリプトを実行して振る舞いを観測する
- テストしにくければ、先にテスタブルな設計へ寄せる
- 作業ブランチは作らない。変更は `master` に直接コミットし、`master` を push する
- 一時的な作業メモは `tmp/` に置く（`tmp/plans/`, `tmp/specs/`）
