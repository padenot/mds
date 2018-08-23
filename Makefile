all:
	cargo build -vvvvv --target=armv7-unknown-linux-gnueabihf
	scp target/armv7-unknown-linux-gnueabihf/debug/monome-bela-seq root@bela.local:~
	ssh root@bela.local ./monome-bela-seq
