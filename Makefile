test:
	cargo xtest

build-kernel:
	cargo xbuild
build-kernel-release:
	cargo xbuild --release

build-image:
	cargo bootimage
build-image-release:
	cargo bootimage --release

burn-image:
	echo dd if=target/x86_64-unknown-bare/debug/bootimage-mtos.bin of=/dev/??? && sync

run: build-image
	qemu-system-x86_64 -drive format=raw,file=target/x86_64-unknown-bare/debug/bootimage-mtos.bin
run-release: build-image-release
	qemu-system-x86_64 -drive format=raw,file=target/x86_64-unknown-bare/release/bootimage-mtos.bin
run-curses: build-image
	echo "ctrl-alt-2 quit"
	qemu-system-x86_64 -display curses -drive format=raw,file=target/x86_64-unknown-bare/debug/bootimage-mtos.bin
#TODO: run headless, connect with spice/vnc
