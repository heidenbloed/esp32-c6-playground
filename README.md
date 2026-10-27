# ESP32-C6 Playground

A `no_std` Embassy firmware starter for the Adafruit ESP32-C6 Feather. The initial
firmware blinks the board's red LED on GPIO15; the NeoPixel and boot button share
GPIO9 and are left untouched.

## Requirements

- Rust stable with the `riscv32imac-unknown-none-elf` target (the repository's
  `rust-toolchain.toml` installs/selects both automatically through rustup)
- [`espflash`](https://github.com/esp-rs/espflash) to flash and monitor the board

Install `espflash` with:

```sh
cargo install espflash
```

## Build, check, and flash

```sh
cargo check
cargo fmt --all -- --check
cargo clippy --target riscv32imac-unknown-none-elf -- -D warnings
cargo build --release
cargo run --release
```

`cargo run --release` uses the configured runner to flash the connected ESP32-C6
and open a serial monitor. Exit the monitor with `Ctrl-C`.

## License

This project is licensed under the MIT License; see [LICENSE](./LICENSE).
