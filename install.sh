#!/usr/bin/env zsh

# zsh
mkdir ~/.zsh
ln -sf ~/dotfiles/.zsh/.zshrc ~/.zsh/.zshrc
ln -sf ~/dotfiles/.zsh/.zshenv ~/.zshenv

# bash
ln -sf ~/dotfiles/.bash/.bash_profile ~/.bash_profile
ln -sf ~/dotfiles/.bash/.bashrc ~/.bashrc

# vim
ln -sf ~/dotfiles/.vimrc ~/.vimrc
