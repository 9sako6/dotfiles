
init: etc/init.sh etc/deploy.sh
	bash etc/init.sh
	bash etc/deploy.sh

deploy: etc/deploy.sh
	bash etc/deploy.sh

clean:
	bash etc/clean.sh
