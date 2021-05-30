#!/bin/bash
# set -e

# Test for deploy scripts
bash -c "$(curl -fsSL https://raw.githubusercontent.com/9sako6/dotfiles/master/etc/init.sh)"
bash -c "$(curl -fsSL https://raw.githubusercontent.com/9sako6/dotfiles/master/etc/clean.sh)"
bash -c "$(curl -fsSL https://raw.githubusercontent.com/9sako6/dotfiles/master/etc/deploy.sh)"
