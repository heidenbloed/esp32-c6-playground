# ESP32-C6 Playground

A `no_std` Embassy firmware starter for the Adafruit ESP32-C6 Feather. The firmware
cycles the onboard RGB NeoPixel through the rainbow using the RMT peripheral. It
enables the NeoPixel's shared power pin (GPIO20) and drives its data pin (GPIO9).
GPIO9 is also shared with the boot button, so the button is unavailable as an input
while the NeoPixel is being driven.

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
