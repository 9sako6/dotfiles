DOTFILES := .bash_profile .bashrc .vimrc .zsh .zshenv mybin

init:
	@bash etc/init.sh
	@make deploy

deploy:
	@bash etc/deploy.sh

clean:
	@bash etc/clean.sh

install-anyenv:
	@git clone https://github.com/riywo/anyenv ~/.anyenv

install-packages-for-rbenv:
	sudo apt-get install -y autoconf bison build-essential libssl-dev libyaml-dev libreadline6-dev zlib1g-dev libncurses5-dev libffi-dev libgdbm3 libgdbm-dev
