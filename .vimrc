" ----------------
" General
" ----------------
syntax enable

" クリップボードにコピーされる
set clipboard+=unnamed

" 左右のカーソル移動で行間移動可能にする。
set whichwrap=b,s,<,>,[,]

" バックスペース有効
set backspace=indent,eol,start

" カーソル行を強調表示
set cursorline
" 縦棒カーソルを使用
" カーソルの形状：https://ttssh2.osdn.jp/manual/ja/usage/tips/vim.html
"
if has('vim_starting')
  " 挿入モード時に点滅の縦棒タイプのカーソル
  "let &t_SI .= "\e]5;CursorShape=1\x7"
  let &t_SI .= "\e[5 q"
  " ノーマルモード時に点滅のブロックタイプのカーソル
  let &t_EI .= "\e[1 q"
  " 置換モード時に点滅の下線タイプのカーソル
  let &t_SR .= "\e[3 q"
endif

" 行番号を表示
set number

" エラー時ビープ音鳴らさない
set noerrorbells

" ビープ音を可視化
set visualbell

" 括弧入力時の対応する括弧を表示
" set showmatch
" 括弧とくオーテーションの自動補完
"inoremap { {}<LEFT>
"inoremap [ []<LEFT>
"inoremap ( ()<LEFT>
"inoremap " ""<LEFT>
"inoremap ' ''<LEFT>

"文字コードをUFT-8に設定
set fenc=utf-8

" バックアップファイルを作らない
set nobackup

" 行末の1文字先までカーソルを移動できるように
set virtualedit=onemore

" 不可視文字を可視化
set list
" タブが「▸-」, 行末半角スペースが「.」
set listchars=tab:\▸\-,trail:.
" 全角スペース可視化
augroup highlightIdegraphicSpace
  autocmd!
  autocmd Colorscheme * highlight IdeographicSpace term=underline ctermbg=DarkGreen guibg=DarkGreen
  autocmd VimEnter,WinEnter * match IdeographicSpace /　/
augroup END
" Tab文字を半角スペースにする
set expandtab

" 行頭以外のTab文字の表示幅（スペースいくつ分）
set tabstop=2

" 行頭でのTab文字の表示幅
set shiftwidth=2

" 検索語をハイライト表示
set hlsearch

" コマンドラインモードでTabによるファイル名補完を有効にする
set wildmenu

" tmux用, 256色
" 本当にこれでいいのだろうか
set t_Co=256

" ----------------
" Key mapping
" ----------------
noremap <S-h> ^
noremap <S-l> $

" ----------------
"  Status Line
" ----------------
" ステータスラインを常に表示
set laststatus=2

" ----------------
" vimplug
" ----------------
call plug#begin()

"
" night-owl
"  - theme
"
Plug 'haishanh/night-owl.vim'

"
" vim-indent-guides
"   - インデントに色を付けて見やすくする
"
Plug 'nathanaelkane/vim-indent-guides'
" vimを立ち上げたときに、自動的にvim-indent-guidesをオンにする
let g:indent_guides_enable_on_vim_startup = 1
" 自動カラー無効
let g:indent_guides_auto_colors=0
" 奇数番目のインデントの色
autocmd VimEnter,Colorscheme * :hi IndentGuidesOdd  ctermbg=235
" 偶数番目のインデントの色
autocmd VimEnter,Colorscheme * :hi IndentGuidesEven ctermbg=235
let g:indent_guides_guide_size = 2

"
" vim-airline
"  - :AirlineTheme THEMENAME
Plug 'vim-airline/vim-airline'
Plug 'vim-airline/vim-airline-themes'
Plug 'jacoborus/tender.vim'
"let g:airline_solarized_bg='light'
let g:airline_theme = 'tender'

call plug#end()

" If you have vim >=8.0 or Neovim >= 0.1.5
if (has("termguicolors"))
 set termguicolors
endif

" For Neovim 0.1.3 and 0.1.4
let $NVIM_TUI_ENABLE_TRUE_COLOR=1

" Theme
syntax enable
colorscheme tender

" Powerline系フォントを利用する
set laststatus=2
let g:airline_powerline_fonts = 1
let g:airline#extensions#tabline#enabled = 1
let g:airline#extensions#tabline#buffer_idx_mode = 1
let g:airline#extensions#whitespace#mixed_indent_algo = 1
"let g:airline_theme = 'tomorrow'
if !exists('g:airline_symbols')
  let g:airline_symbols = {}
endif

" unicode symbols
let g:airline_left_sep = '»'
let g:airline_left_sep = '▶'
let g:airline_right_sep = '«'
let g:airline_right_sep = '◀'
let g:airline_symbols.crypt = '🔒'
let g:airline_symbols.linenr = '☰'
let g:airline_symbols.linenr = '␊'
let g:airline_symbols.linenr = '␤'
let g:airline_symbols.linenr = '¶'
let g:airline_symbols.maxlinenr = ''
let g:airline_symbols.maxlinenr = '㏑'
let g:airline_symbols.branch = '⎇'
let g:airline_symbols.paste = 'ρ'
let g:airline_symbols.paste = 'Þ'
let g:airline_symbols.paste = '∥'
let g:airline_symbols.spell = 'Ꞩ'
let g:airline_symbols.notexists = '∄'
let g:airline_symbols.whitespace = 'Ξ'

" powerline symbols
let g:airline_left_sep = ''
let g:airline_left_alt_sep = ''
let g:airline_right_sep = ''
let g:airline_right_alt_sep = ''
let g:airline_symbols.branch = ''
let g:airline_symbols.readonly = ''
let g:airline_symbols.linenr = '☰'
let g:airline_symbols.maxlinenr = ''

" 入力補完メニューの色
"   - ctermfg  : 文字の色
"   - ctermbg  : 背景色
"   - Pmenu    : 選択されていない候補
"   - PmenuSel : 選択されている候補
highlight Pmenu ctermfg=white ctermbg=darkgray
highlight PmenuSel ctermfg=white ctermbg=blue

" 括弧ハイライトの色
highlight MatchParen ctermfg=LightGreen ctermbg=black
