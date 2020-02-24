" Python 3.x系のPathを設定
" let g:python3_host_prog = system('echo -n (which python3)')

set noerrorbells       " エラー時ビープ音鳴らさない
set autoindent         " 改行時に自動でインデントする
set tabstop=2          " タブを何文字の空白に変換するか
set shiftwidth=2       " 自動インデント時に入力する空白の数
set expandtab          " タブ入力を空白に変換
set splitright         " 画面を縦分割する際に右に開く
set clipboard=unnamed  " yank した文字列をクリップボードにコピー
set hls                " 検索した文字をハイライトする

" 左右のカーソル移動で行間移動可能にする。
set whichwrap=b,s,<,>,[,]

" バックスペース有効
set backspace=indent,eol,start

"文字コードをUFT-8に設定
set fenc=utf-8

" バックアップファイルを作らない
set nobackup

" 行末の1文字先までカーソルを移動できるように
set virtualedit=onemore

" 不可視文字を可視化
" todo

" tmux用, 256色
" 本当にこれでいいのだろうか
set t_Co=256

" dein.vim を使うために以下を記述
runtime! plugins/dein.rc.vimj

" ----------------
" Key mapping
" ----------------
noremap <S-h> ^
noremap <S-l> $