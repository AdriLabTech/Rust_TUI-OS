.PHONY: setup image qemu screenshot dump test clean
.EXPORT_ALL_VARIABLES:

# ---------------------------------------------------------------------------
# TUI-OS: kernel + bootloader + built-in ratatui desktop (no userland).
# ---------------------------------------------------------------------------

setup:
	rustup toolchain install nightly-2026-03-01 --profile minimal
	cargo install bootimage

# Compilation options
output = video          # video, serial
mode = release

# Keyboard layout compiled into the kernel. The PS/2 driver reads this with
# option_env! at build time, so it is baked in rather than chosen at runtime.
# `image` always touches src/lib.rs, so changing this really does rebuild.
#
# The guard is not decoration: `init` discards `set_keyboard`'s result, so a
# name the decoder does not know would build a machine whose keyboard never
# works, with nothing printed to say why.
keyboard = qwerty
known-keyboards = azerty dvorak qwerty
ifeq ($(filter $(keyboard),$(known-keyboards)),)
$(error keyboard must be one of: $(known-keyboards) -- got '$(keyboard)')
endif
export TUIOS_KEYBOARD = $(keyboard)

# Emulation options
arch = x86_64
cpu = core2duo
smp = 2
memory = 32
bootwait = 8
kvm = false
monitor = false

export TUIOS_VERSION = $(shell v=$$(git describe --tags --match 'tuios-*' 2>/dev/null | sed 's/^v//'); echo "$${v:-0.1.0}")

bin = tuios-$(arch).img
img = disk.img

cargo-opts = --bin tuios
ifeq ($(mode),release)
cargo-opts += --release
endif
cargo-opts += --no-default-features --features $(output)

$(img):
	qemu-img create $(img) 32M

# Rebuild TUI-OS if the features list changed
image: $(img)
	touch src/lib.rs
	cargo bootimage $(cargo-opts)
	cp target/x86_64-tuios/$(mode)/bootimage-tuios.bin $(bin)
	dd conv=notrunc if=$(bin) of=$(img)

qemu-opts = -name "TUI-OS $$TUIOS_VERSION" \
			-m $(memory) -smp $(smp) \
			-drive file=$(img),format=raw

ifeq ($(kvm),true)
qemu-opts += -cpu host -accel kvm
else
qemu-opts += -cpu $(cpu)
endif

ifeq ($(monitor),true)
qemu-opts += -monitor telnet:127.0.0.1:7777,server,nowait
endif

ifeq ($(arch),i686)
qemu = qemu-system-i386
cpu = n270
smp = 1
else
qemu = qemu-system-x86_64
endif

ifeq ($(mode),debug)
qemu-opts += -s -S
endif

qemu:
	$(qemu) $(qemu-opts)

# Dump the current screen to /tmp/tuios.ppm (needs the telnet monitor)
screenshot:
	printf 'screendump /tmp/tuios.ppm\nquit\n' | timeout 1 bash -c 'exec 3<>/dev/tcp/127.0.0.1/7777; cat >&3; cat <&3' || true
	convert /tmp/tuios.ppm /tmp/tuios.png
	echo "Saved /tmp/tuios.png"

# Read the 80x25 VGA text buffer back as text. Boots headless with a QMP
# socket, waits for the desktop, then decodes 0xB8000. Pass KEYS="l s ret" to
# type before dumping, COLORS=true to mark tinted cells.
qmp-sock = /tmp/tuios-qmp.sock
qmp-pid = /tmp/tuios-qmp.pid

# QEMU is killed by the pid we recorded, and never by name. `pkill -f
# qemu-system-$(arch)` looks like the obvious way to do it and is wrong: `-f`
# matches whole command lines, and that pattern is part of *this recipe's* own
# command line, so pkill signals the shell running the recipe. Make then reports
# `Terminado` and exits 2 after a dump that was perfectly fine -- a failure with
# no cause behind it. Two rules, then: record the pid, and let vgatext's exit
# status be make's, so a dump that really did fail fails the build too.
dump:
	rm -f $(qmp-sock) $(qmp-pid)
	$(qemu) $(qemu-opts) -qmp unix:$(qmp-sock),server,nowait -display none & echo $$! > $(qmp-pid)
	sleep $(bootwait)
	python3 tools/vgatext.py --socket $(qmp-sock) $(if $(KEYS),--keys "$(KEYS)",) $(if $(COLORS),--colors,); status=$$?; kill $$(cat $(qmp-pid)) 2>/dev/null || true; rm -f $(qmp-pid); exit $$status

test-opts = --lib
ifeq ($(mode),release)
test-opts += --release
endif

test:
	cargo test $(test-opts) --no-default-features --features serial -- \
		-m $(memory) -cpu $(cpu) -display none -serial stdio \
		-device isa-debug-exit,iobase=0xF4,iosize=0x04

clean:
	cargo clean
	rm -f tuios-$(arch).img disk.img