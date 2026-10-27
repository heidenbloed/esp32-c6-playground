# ESP32-C6 Playground

A `no_std` Embassy firmware starter for the Adafruit ESP32-C6 Feather. The onboard
RGB NeoPixel indicates the battery charge level from the MAX17048 fuel gauge:
red means low, yellow means mid-charge, and green means full. The level is refreshed
after each servo sweep cycle (about every 3.4 seconds). The gauge is read over I²C
(SDA GPIO19, SCL GPIO18).

The firmware enables the NeoPixel and STEMMA QT power pin (GPIO20) and drives the
NeoPixel data pin (GPIO9). GPIO9 is also shared with the boot button, so the button
is unavailable as an input while the NeoPixel is being driven. The board's separate
yellow CHG LED indicates active charging; the MAX17048 reports battery level, not
whether charging is currently active.

An SG90 servo signal can be connected to A3 (GPIO5). The firmware maps requested
angles from 0° to 180° to approximately 1.0-2.0 ms pulses at 50 Hz. Servo endpoints
vary by model and may need calibration; stop if it strains or buzzes at an endpoint.
An SG90 is a position servo, not a continuous-rotation motor. When trying it from the
LiPo, connect servo power to BAT and ground to GND; at the battery's 3.0-4.2 V range,
operation may be weak or unreliable because SG90 servos are typically specified for
4.8-6 V.

## Wi-Fi servo control

The board joins a 2.4 GHz Wi-Fi network and serves a simple angle slider on port 80.
Copy `.wifi.env.example` to `.wifi.env` and set your network's SSID and password:

```sh
cp .wifi.env.example .wifi.env
```

`.wifi.env` is git-ignored; credentials are embedded in the firmware at build time.
After boot, read the assigned address from the serial output and open
`http://<board-ip>/` on a device on the same local network. The unauthenticated page
allows any device on that network to set the servo angle from 0° to 180°. Do not
forward port 80 from the internet; use this only on a trusted LAN.

## Requirements

- Rust stable with the `riscv32imac-unknown-none-elf` target and `rust-analyzer`
  component (the repository's `rust-toolchain.toml` installs/selects them through
  rustup)
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
