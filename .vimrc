" ----------------
" General
" ----------------
syntax enable

" 左右のカーソル移動で行間移動可能にする。
set whichwrap=b,s,<,>,[,]

" 行番号を表示
set number

" エラー時ビープ音鳴らさない
set noerrorbells

" ビープ音を可視化
set visualbell

" 括弧入力時の対応する括弧を表示
" set showmatch

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
autocmd VimEnter,Colorscheme * :hi IndentGuidesEven ctermbg=16
let g:indent_guides_guide_size = 2

"
" vim-airline
"  - :AirlineTheme THEMENAME
Plug 'vim-airline/vim-airline'
Plug 'vim-airline/vim-airline-themes'
let g:airline_solarized_bg='dark'
let g:airline_theme = 'wombat'
call plug#end()

colorscheme night-owl
