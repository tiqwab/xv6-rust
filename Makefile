.PHONY: clean build

build:
	cargo xbuild --target i686-xv6rust.json
