> [!NOTE]
> このファイルは `obra/superpowers` の `skills/brainstorming/visual-companion.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Visual Companion Guide

mockup、diagram、選択肢を表示するための、ブラウザベースの visual brainstorming companion。

## 使うタイミング

判断はセッション単位ではなく質問単位で行う。基準は **読むより見たほうがユーザーが理解しやすいか** である。

**ブラウザを使う** のは、内容そのものが視覚的なとき:

- **UI mockup** — wireframe、layout、navigation structure、component design
- **architecture diagram** — system component、data flow、relationship map
- **横並びの visual comparison** — 2 つの layout、2 つの color scheme、2 つの design direction の比較
- **design の磨き込み** — look and feel、spacing、visual hierarchy が論点のとき
- **空間的な関係** — state machine、flowchart、diagram として描画された entity relationship

**端末を使う** のは、内容がテキストまたは表形式のとき:

- **要件とスコープの質問** — "what does X mean?"、"which features are in scope?"
- **概念的な A/B/C の選択** — 言葉で説明されるアプローチの選択
- **トレードオフ一覧** — pros/cons、比較表
- **技術判断** — API design、data modeling、architectural approach の選択
- **確認質問** — 答えが視覚的好みではなく言葉であるもの全般

UI に関する質問 *である* ことは、自動的に visual question であることを意味しない。"What kind of wizard do you want?" は概念的であり、端末を使う。"Which of these wizard layouts feels right?" は視覚的であり、ブラウザを使う。

## 仕組み

server は HTML ファイルのあるディレクトリを監視し、最新のものをブラウザに配信する。HTML content を `screen_dir` に書き、ユーザーはブラウザでそれを見て、クリックで選択肢を選べる。選択は `state_dir/events` に記録され、次の turn でそれを読む。

**content fragment と full document:** HTML ファイルが `<!DOCTYPE` または `<html` で始まる場合、server はそれをそのまま配信する（helper script だけ挿入する）。そうでなければ、server は自動的に content を frame template で包み、header、CSS theme、selection indicator、すべての interactive infrastructure を追加する。**デフォルトでは content fragment を書くこと。** ページ全体を完全に制御する必要がある場合だけ full document を書く。

## セッションを開始する

```bash
# 永続化つきで server を起動する（mockup を project に保存する）
scripts/start-server.sh --project-dir /path/to/project

# Returns: {"type":"server-started","port":52341,"url":"http://localhost:52341",
#           "screen_dir":"/path/to/project/.superpowers/brainstorm/12345-1706000000/content",
#           "state_dir":"/path/to/project/.superpowers/brainstorm/12345-1706000000/state"}
```

返答から `screen_dir` と `state_dir` を保存する。ユーザーには URL を開くよう伝える。

**接続情報の見つけ方:** server は起動時 JSON を `$STATE_DIR/server-info` に書く。バックグラウンドで起動して stdout を捕まえなかった場合は、そのファイルを読んで URL と port を得る。`--project-dir` を使っている場合は、セッションディレクトリを `<project>/.superpowers/brainstorm/` で確認する。

**注意:** mockup が `.superpowers/brainstorm/` に永続化され、server restart 後も残るように、project root を `--project-dir` として渡す。これを付けないとファイルは `/tmp` に置かれ、掃除される。まだ入っていないなら `.superpowers/` を `.gitignore` に追加するようユーザーに案内する。

**platform ごとの server 起動方法:**

**Claude Code (macOS / Linux):**
```bash
# デフォルトモードでよい。script 自身が server をバックグラウンド化する
scripts/start-server.sh --project-dir /path/to/project
```

**Claude Code (Windows):**
```bash
# Windows は自動判定されて foreground mode を使うため、tool call を block する。
# Bash tool 呼び出しに run_in_background: true を付けて、
# server が会話 turn をまたいで生き残るようにする。
scripts/start-server.sh --project-dir /path/to/project
```
Bash tool 経由で呼び出す場合は `run_in_background: true` を設定する。その後の turn で `$STATE_DIR/server-info` を読んで URL と port を取得する。

**Codex:**
```bash
# Codex は background process を reap する。script は CODEX_CI を自動判定して
# foreground mode に切り替える。通常どおり実行すればよい。追加 flag は不要。
scripts/start-server.sh --project-dir /path/to/project
```

**Gemini CLI:**
```bash
# --foreground を使い、shell tool call に is_background: true を設定して
# process が turn をまたいで生き残るようにする
scripts/start-server.sh --project-dir /path/to/project --foreground
```

**その他の環境:** server は会話 turn をまたいでバックグラウンドで動き続けなければならない。環境が detached process を reap するなら、`--foreground` を使い、その platform の background execution 手段でコマンドを起動する。

ブラウザから URL に到達できない場合（remote / containerized な環境でよくある）、非 loopback host に bind する:

```bash
scripts/start-server.sh \
  --project-dir /path/to/project \
  --host 0.0.0.0 \
  --url-host localhost
```

返ってくる URL JSON にどの hostname を表示するかは `--url-host` で制御する。

## ループ

1. **server が生きているか確認し**、その後 **新しい HTML を `screen_dir` に書く**:
   - 各書き込み前に `$STATE_DIR/server-info` が存在するか確認する。存在しない（または `$STATE_DIR/server-stopped` が存在する）なら、server は停止している。続ける前に `start-server.sh` で再起動する。server は 30 分間操作がないと自動終了する。
   - 意味のある filename を使う: `platform.html`、`visual-style.html`、`layout.html`
   - **filename を再利用してはならない** — 各 screen は新しい file を使う
   - Write tool を使う — **cat/heredoc は使わない**（端末にノイズを吐く）
   - server は自動的に最新 file を配信する

2. **ユーザーに何が起きるか伝えて turn を終える:**
   - URL を毎回伝える（最初の一回だけではない）
   - 画面に何が表示されているかを簡潔に要約する（例: 「ホームページの 3 つの layout option を表示しています」）
   - 端末で返答するよう依頼する: "Take a look and let me know what you think. Click to select an option if you'd like."

3. **次の turn で** — ユーザーが端末で返答した後:
   - `$STATE_DIR/events` が存在すれば読む — これにはユーザーのブラウザ操作（click、selection）が JSON Lines で入っている
   - それをユーザーの端末テキストと組み合わせて全体像を把握する
   - 端末メッセージが主要なフィードバックであり、`state_dir/events` は構造化された操作データを補助する

4. **反復するか先へ進む** — フィードバックが現在の screen を変えるなら、新しい file（例: `layout-v2.html`）を書く。現在の step が検証されるまでは次の質問に進まない。

5. **端末に戻るときは unload する** — 次の step でブラウザが不要な場合（例: 確認質問、トレードオフの議論）、待機 screen を push して古い content を消す:

   ```html
   <!-- filename: waiting.html (or waiting-2.html, etc.) -->
   <div style="display:flex;align-items:center;justify-content:center;min-height:60vh">
     <p class="subtitle">Continuing in terminal...</p>
   </div>
   ```

   これにより、会話が次に進んでいるのに、ユーザーが解決済みの選択肢を見続けることを防ぐ。次の visual question が来たら、通常どおり新しい content file を push する。

6. 終了まで繰り返す。

## Content Fragment を書く

ページの中に入る content だけを書く。server が自動で frame template（header、theme CSS、selection indicator、すべての interactive infrastructure）で包む。

**最小例:**

```html
<h2>Which layout works better?</h2>
<p class="subtitle">Consider readability and visual hierarchy</p>

<div class="options">
  <div class="option" data-choice="a" onclick="toggleSelect(this)">
    <div class="letter">A</div>
    <div class="content">
      <h3>Single Column</h3>
      <p>Clean, focused reading experience</p>
    </div>
  </div>
  <div class="option" data-choice="b" onclick="toggleSelect(this)">
    <div class="letter">B</div>
    <div class="content">
      <h3>Two Column</h3>
      <p>Sidebar navigation with main content</p>
    </div>
  </div>
</div>
```

これで十分。`<html>` も CSS も `<script>` tag も不要。server がそれらを提供する。

## 利用できる CSS Class

frame template は content 用に次の CSS class を提供する:

### Options（A/B/C の選択肢）

```html
<div class="options">
  <div class="option" data-choice="a" onclick="toggleSelect(this)">
    <div class="letter">A</div>
    <div class="content">
      <h3>Title</h3>
      <p>Description</p>
    </div>
  </div>
</div>
```

**複数選択:** 複数選択を許可したい場合は、container に `data-multiselect` を追加する。各 click は item の切り替えになる。indicator bar は件数を表示する。

```html
<div class="options" data-multiselect>
  <!-- same option markup — users can select/deselect multiple -->
</div>
```

### Cards（visual design）

```html
<div class="cards">
  <div class="card" data-choice="design1" onclick="toggleSelect(this)">
    <div class="card-image"><!-- mockup content --></div>
    <div class="card-body">
      <h3>Name</h3>
      <p>Description</p>
    </div>
  </div>
</div>
```

### Mockup Container

```html
<div class="mockup">
  <div class="mockup-header">Preview: Dashboard Layout</div>
  <div class="mockup-body"><!-- your mockup HTML --></div>
</div>
```

### Split View（横並び）

```html
<div class="split">
  <div class="mockup"><!-- left --></div>
  <div class="mockup"><!-- right --></div>
</div>
```

### Pros/Cons

```html
<div class="pros-cons">
  <div class="pros"><h4>Pros</h4><ul><li>Benefit</li></ul></div>
  <div class="cons"><h4>Cons</h4><ul><li>Drawback</li></ul></div>
</div>
```

### Mock Element（wireframe の部品）

```html
<div class="mock-nav">Logo | Home | About | Contact</div>
<div style="display: flex;">
  <div class="mock-sidebar">Navigation</div>
  <div class="mock-content">Main content area</div>
</div>
<button class="mock-button">Action Button</button>
<input class="mock-input" placeholder="Input field">
<div class="placeholder">Placeholder area</div>
```

### タイポグラフィと section

- `h2` — ページタイトル
- `h3` — section 見出し
- `.subtitle` — title 下の補助テキスト
- `.section` — 下マージン付きの content block
- `.label` — 小さな大文字ラベル

## ブラウザイベントの形式

ユーザーがブラウザで option を click すると、その操作は `$STATE_DIR/events` に記録される（1 行につき 1 つの JSON object）。新しい screen を push すると file は自動で消去される。

```jsonl
{"type":"click","choice":"a","text":"Option A - Simple Layout","timestamp":1706000101}
{"type":"click","choice":"c","text":"Option C - Complex Grid","timestamp":1706000108}
{"type":"click","choice":"b","text":"Option B - Hybrid","timestamp":1706000115}
```

完全な event stream は、ユーザーがどのように探索したかを示す。最終的に決める前に複数 option を click することがある。最後の `choice` event が最終選択であることが多いが、click の並び自体が、迷いや好みを示す手がかりになる。

`$STATE_DIR/events` が存在しない場合、ユーザーはブラウザで操作していない。その場合は端末テキストだけを使う。

## 設計のヒント

- **忠実度は質問に合わせる** — layout なら wireframe、polish の質問なら polish を出す
- **各ページで問いを説明する** — "Pick one" ではなく "Which layout feels more professional?"
- **先へ進む前に反復する** — フィードバックが current screen を変えるなら新しい version を書く
- **1 screen あたり 2〜4 option まで**
- **必要なら実コンテンツを使う** — たとえば写真ポートフォリオなら actual image（Unsplash）を使う。placeholder content は設計上の問題を見えにくくする。
- **mockup は単純に保つ** — pixel-perfect design ではなく、layout と structure に集中する

## ファイル命名

- 意味のある名前を使う: `platform.html`、`visual-style.html`、`layout.html`
- filename を再利用してはならない — 各 screen は新しい file でなければならない
- 反復版には `layout-v2.html`、`layout-v3.html` のように version suffix を付ける
- server は更新時刻で最も新しい file を配信する

## 後片付け

```bash
scripts/stop-server.sh $SESSION_DIR
```

セッションで `--project-dir` を使っていた場合、mockup file は後で参照できるよう `.superpowers/brainstorm/` に残る。stop 時に削除されるのは `/tmp` セッションだけである。

## 参考

- Frame template（CSS reference）: `scripts/frame-template.html`
- Helper script（client-side）: `scripts/helper.js`
