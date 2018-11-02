all:
	cargo build -vvvvv --target=armv7-unknown-linux-gnueabihf --release
	arm-bela-linux-gnueabihf-strip target/armv7-unknown-linux-gnueabihf/release/monome-bela-seq
	scp target/armv7-unknown-linux-gnueabihf/debug/monome-bela-seq root@bela.local:~
	ssh root@bela.local ./monome-bela-seq
