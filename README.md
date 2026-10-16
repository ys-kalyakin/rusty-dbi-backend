# dbi-backend

A Rust USB backend for [DBI](https://github.com/rashevskyv/dbi), a Nintendo Switch
homebrew app for installing titles over USB. This is a reimplementation of the
Python `dbibackend.py`.

It listens for the Switch over USB (VID `0x057E`, PID `0x3000`), answers the DBI0
protocol commands `LIST`, `FILE_RANGE`, and `EXIT`, and streams the requested
byte ranges of your NSP files.

## Build

```bash
cargo build --release
```

## Usage

```bash
# Make a directory's files available and open the TUI
dbi-backend ~/games

# Or pass specific files
dbi-backend game1.nsp game2.nsp
```

Keys: `a` add a path, `c` clear the list, `s` start the server, `q` quit.

## Linux permissions

Opening the USB device requires access to the `/dev/bus/usb/...` node. Install the
provided udev rule and add your user to the `plugdev` group:

```bash
sudo cp contrib/99-nintendo-switch.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
sudo usermod -aG plugdev "$USER"   # log out and back in
```

## Protocol

Messages use a 16-byte little-endian header: `DBI0` magic, `cmd_type`, `cmd_id`,
`data_size`. See `crates/core/src/protocol.rs`.

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```
