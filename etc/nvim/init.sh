if [ ! -e "${HOME}/.config/nvim" ]; then
  if [ ! -e "${HOME}/.config" ]; then
    mkdir "${HOME}/.config"
  fi
  ln -sv "${DOTFILES_PATH}/.config/nvim" "${HOME}/.config"
fi