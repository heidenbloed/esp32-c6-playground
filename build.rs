use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=.wifi.env");
    println!("cargo:rerun-if-env-changed=WIFI_SSID");
    println!("cargo:rerun-if-env-changed=WIFI_PASSWORD");

    let file = if Path::new(".wifi.env").exists() {
        fs::read_to_string(".wifi.env").expect("Failed to read .wifi.env")
    } else {
        String::new()
    };

    let ssid = env::var("WIFI_SSID")
        .ok()
        .or_else(|| credential_from_file(&file, "WIFI_SSID"))
        .expect("Set WIFI_SSID in the build environment or .wifi.env");
    let password = env::var("WIFI_PASSWORD")
        .ok()
        .or_else(|| credential_from_file(&file, "WIFI_PASSWORD"))
        .expect("Set WIFI_PASSWORD in the build environment or .wifi.env");

    assert!(
        !ssid.is_empty() && ssid.len() <= 32,
        "WIFI_SSID must be 1-32 bytes"
    );
    assert!(
        !password.is_empty() && password.len() <= 64,
        "WIFI_PASSWORD must be 1-64 bytes"
    );

    println!("cargo:rustc-env=WIFI_SSID={ssid}");
    println!("cargo:rustc-env=WIFI_PASSWORD={password}");

    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

fn credential_from_file(file: &str, key: &str) -> Option<String> {
    file.lines().find_map(|line| {
        let (name, value) = line.trim().split_once('=')?;
        (name.trim() == key).then(|| value.trim().to_owned())
    })
}
