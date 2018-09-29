# _   .-')      ('-.    .-. .-')     ('-.                                 ('-.   
# ( '.( OO )_   ( OO ).-.\  ( OO )  _(  OO)                              _(  OO)  
#  ,--.   ,--.) / . --. /,--. ,--. (,------.   ,------.,-.-')  ,--.     (,------. 
#  |   `.'   |  | \-.  \ |  .'   /  |  .---'('-| _.---'|  |OO) |  |.-')  |  .---' 
#  |         |.-'-'  |  ||      /,  |  |    (OO|(_\    |  |  \ |  | OO ) |  |     
#  |  |'.'|  | \| |_.'  ||     ' _)(|  '--. /  |  '--. |  |(_/ |  |`-' |(|  '--.  
#  |  |   |  |  |  .-.  ||  .   \   |  .--' \_)|  .--',|  |_.'(|  '---.' |  .--'  
#  |  |   |  |  |  | |  ||  |\   \  |  `---.  \|  |_)(_|  |    |      |  |  `---. 
#  `--'   `--'  `--' `--'`--' '--'  `------'   `--'    `--'    `------'  `------' 

init: ## Initialize settings for dotfiles
	@bash etc/init.sh
	@make deploy

deploy: ## Deploy dotfiles
	@bash etc/deploy.sh

clean: ## Clean dotfiles' settings
	@bash etc/clean.sh

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[32m%-32s\033[0m %s\n", $$1, $$2}'

#
# git
#   - config
#
git-config: ## Set up Git configs (email, name, commit.template)
	git config --global user.email "31821663+9sako6@users.noreply.github.com"
	git config --global user.name "9sako6"
	git config --global commit.template ~/.commit_template

#
# git
#   - version up
#
git-version-up: ## Update Git
	sudo apt-get install software-properties-common
	sudo add-apt-repository ppa:git-core/ppa
	sudo apt-get update
	sudo apt-get install git
	git version

#
# anyenv
#   - installation
#
install-anyenv: ## Install anyenv
	@git clone https://github.com/riywo/anyenv ~/.anyenv

#
# ruby-build
#   - needed to install ruby via rbenv
#
# Suggested build environment for Ubuntu
# https://github.com/rbenv/ruby-build/wiki
install-packages-for-ruby-build: ## Install some packages for ruby-build (needed: ruby via rbenv)
	sudo apt-get install -y autoconf bison build-essential libssl-dev libyaml-dev libreadline6-dev zlib1g-dev libncurses5-dev libffi-dev libgdbm3 libgdbm-dev
