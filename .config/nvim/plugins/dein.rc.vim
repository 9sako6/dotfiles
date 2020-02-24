" プラグインの設定ファイルPath
let s:plugin = '~/.config/nvim/plugins/config/dein.toml'

if &compatible
  set nocompatible
endif

set runtimepath+=~/.cache/dein/repos/github.com/Shougo/dein.vim

if dein#load_state('~/.cache/dein')
  call dein#begin('~/.cache/dein')
  " dein.tomlを起動時ロードの設定ファイルとして読み込む
  call dein#load_toml(s:plugin, {'lazy': 0})
  call dein#end()
  call dein#save_state()
endif

" Neovim起動時にdein.tomlファイルをチェックし、未インストールのプラグインがあった場合インストールする
if dein#check_install()
  call dein#install()
endif

filetype plugin indent on
syntax enable