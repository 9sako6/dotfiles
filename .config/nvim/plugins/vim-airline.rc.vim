syntax enable

" Theme
let g:airline_theme = 'molokai'
colorscheme molokai

" Powerline系フォントを利用する
set laststatus=2
let g:airline_powerline_fonts = 1
let g:airline#extensions#tabline#enabled = 1
let g:airline#extensions#tabline#buffer_idx_mode = 1
let g:airline#extensions#whitespace#mixed_indent_algo = 1

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

" iTerm2で半透明にしているが、vimのcolorschemeを設定すると背景も変更されるため
highlight Normal ctermbg=none

"
" 行番号
"
" 行番号を表示
set number
" 現在の行を強調表示
set cursorline
" 現在の行の下線を非表示
hi clear CursorLine
"autocmd ColorScheme * highlight CursorLineNr term=bold   cterm=NONE ctermfg=14 ctermbg=NONE
"autocmd ColorScheme * highlight LineNr ctermfg=7 ctermbg=8
"autocmd ColorScheme * highlight LineNr ctermfg=239

"
" original color setting
"
" コメント
autocmd ColorScheme * highlight Comment ctermfg=102
" ビジュアルモードの選択範囲
autocmd ColorScheme * highlight Visual ctermbg=236
