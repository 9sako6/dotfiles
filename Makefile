DOTFILES := .bash_profile .bashrc .vimrc .zsh .zshenv mybin

init:
	@bash etc/init.sh
	@make deploy

deploy:
	@bash etc/deploy.sh

clean:
	@bash etc/clean.sh

#
# git
#   - version up
#
git-version-up:
	git version
	sudo apt-get install software-properties-common
	sudo add-apt-repository ppa:git-core/ppa
	sudo apt-get update
	sudo apt-get install git
	git version

#
# anyenv
#   - installation
#
install-anyenv:
	@git clone https://github.com/riywo/anyenv ~/.anyenv

#
# ruby-build
#   - needed to install ruby via rbenv
#
# Suggested build environment for Ubuntu
# https://github.com/rbenv/ruby-build/wiki
install-packages-for-ruby-build:
	sudo apt-get install -y autoconf bison build-essential libssl-dev libyaml-dev libreadline6-dev zlib1g-dev libncurses5-dev libffi-dev libgdbm3 libgdbm-dev
