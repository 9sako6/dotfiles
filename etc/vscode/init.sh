source "${HOME}/dotfiles/etc/env.sh"
source "${DOTFILES_PATH}/etc/utils.sh"

vscode_files=(snippets keybindings.json settings.json)
vscode_user_path="${HOME}/Library/Application Support/Code/User"
for f in ${vscode_files[@]}; do
  if [ -e "${vscode_user_path}" ] && [ ! -e "${vscode_user_path}/${f}" ]; then
    ln -sv "${DOTFILES_PATH}/dist/.vscode/${f}" "${vscode_user_path}"
  fi
done

# install extensions
if type code >/dev/null 2>&1; then
  cat "${DOTFILES_PATH}/dist/.vscode/extensions" | while read line; do
    code --install-extension $line
  done
else
  echo "Please install code command for VSCode"
fi
