#!/bin/bash

# clean
for f in .??*
do
    [[ "$f" == ".git" ]] && continue
    [[ "$f" == ".gitignore" ]] && continue
    [[ "$f" == ".DS_Store" ]] && continue
    [[ "$f" == ".vim" ]] && continue
    if [ -d $f ]; then
      rm -r ~/$f
    else
      rm ~/$f
    fi
done
rm -r ~/mybin

echo -e "\033[0;32mdotfiles are successfully cleaned!\033[0m"
