DOTFILES := .bash_profile .bashrc .vimrc .zsh .zshenv mybin

init:
	@bash etc/init.sh
	@make deploy

deploy:
	@bash etc/deploy.sh

clean:
	@bash etc/clean.sh
