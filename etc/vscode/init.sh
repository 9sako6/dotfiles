vscode_files=( snippets keybindings.json settings.json )
vscode_user_path="${HOME}/Library/Application Support/Code/User"
for f in ${vscode_files[@]}
do
  if [ -e "${vscode_user_path}" ] && [ ! -e "${vscode_user_path}/${f}" ]; then
    ln -sv "${HOME}/dotfiles/.vscode/${f}" "${vscode_user_path}"
  fi
done

# install extensions
cat "${HOME}/dotfiles/.vscode/extensions" | while read line
do
  code --install-extension $line
done

