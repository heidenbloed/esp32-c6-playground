#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use core::sync::atomic::{AtomicU8, Ordering};

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embedded_hal::pwm::SetDutyCycle;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{Level, Output, OutputConfig},
    i2c::master::{Config as I2cConfig, I2c},
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed,
        channel::{self, ChannelIFace},
        timer::{self, TimerIFace},
    },
    rmt::Rmt,
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_hal_smartled::SmartLedsAdapter;
use smart_leds::{
    SmartLedsWrite, brightness, gamma,
    hsv::{Hsv, hsv2rgb},
};
use static_cell::{ConstStaticCell, StaticCell};

esp_bootloader_esp_idf::esp_app_desc!();

static SERVO_TARGET_DEGREES: AtomicU8 = AtomicU8::new(0);
static NETWORK_RESOURCES: ConstStaticCell<embassy_net::StackResources<2>> =
    ConstStaticCell::new(embassy_net::StackResources::new());
static RMT_BUFFER: ConstStaticCell<[esp_hal::rmt::PulseCode; esp_hal_smartled::buffer_size(1)]> =
    ConstStaticCell::new([esp_hal::rmt::PulseCode::end_marker(); esp_hal_smartled::buffer_size(1)]);
static HTTP_RX_BUFFER: ConstStaticCell<[u8; 512]> = ConstStaticCell::new([0; 512]);
static HTTP_TX_BUFFER: ConstStaticCell<[u8; 512]> = ConstStaticCell::new([0; 512]);
static HTTP_REQUEST_BUFFER: ConstStaticCell<[u8; 512]> = ConstStaticCell::new([0; 512]);

const INDEX_RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\
Content-Type: text/html; charset=utf-8\r\n\
Connection: close\r\n\
Cache-Control: no-store\r\n\
X-Content-Type-Options: nosniff\r\n\
\r\n\
<!doctype html><html lang=\"en\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>Servo control</title><style>body{font:1.2rem system-ui;max-width:32rem;margin:3rem auto;padding:0 1rem}\
input{width:100%}output{font-weight:bold}</style><h1>Servo angle</h1>\
<label for=\"angle\">Angle: <output id=\"value\">0</output>&deg;</label>\
<input id=\"angle\" type=\"range\" min=\"0\" max=\"180\" value=\"0\">\
<p id=\"status\">Move the slider to set the servo.</p>\
<script>const s=document.querySelector('#angle'),v=document.querySelector('#value'),m=document.querySelector('#status');\
s.oninput=()=>v.value=s.value;s.onchange=async()=>{m.textContent='Applying...';try{const r=await fetch('/servo?angle='+s.value);\
m.textContent=r.ok?'Servo set to '+s.value+' degrees':'Request failed ('+r.status+')';}catch{m.textContent='Could not reach the board.';}};</script>";

const NO_CONTENT_RESPONSE: &[u8] =
    b"HTTP/1.1 204 No Content\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n";
const BAD_REQUEST_RESPONSE: &[u8] =
    b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
const NOT_FOUND_RESPONSE: &[u8] =
    b"HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);

    let _neopixel_power = Output::new(peripherals.GPIO20, Level::High, OutputConfig::default());
    let mut i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_khz(100)),
    )
    .expect("Failed to initialize I2C")
    .with_sda(peripherals.GPIO19)
    .with_scl(peripherals.GPIO18);

    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80))
        .expect("Failed to initialize the RMT peripheral");
    let rmt_buffer = RMT_BUFFER.take();
    let mut led = SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO9, rmt_buffer);

    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
    let mut servo_timer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    servo_timer
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty12Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_hz(50),
        })
        .expect("Failed to configure the servo PWM timer");

    let mut servo = ledc.channel(channel::Number::Channel0, peripherals.GPIO5);
    servo
        .configure(channel::config::Config {
            timer: &servo_timer,
            duty_pct: 0,
            drive_mode: esp_hal::gpio::DriveMode::PushPull,
        })
        .expect("Failed to configure the servo PWM channel");

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let software_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, software_interrupt.software_interrupt0);

    esp_println::println!("Initializing Wi-Fi");
    static RADIO: StaticCell<esp_radio::Controller<'static>> = StaticCell::new();
    let radio = RADIO.init(esp_radio::init().expect("Failed to initialize the Wi-Fi radio"));
    let (mut wifi, interfaces) = esp_radio::wifi::new(radio, peripherals.WIFI, Default::default())
        .expect("Failed to create the Wi-Fi station");
    configure_wifi(&mut wifi);
    wifi.start_async().await.expect("Failed to start Wi-Fi");
    esp_println::println!("Connecting to Wi-Fi");
    wifi.connect_async()
        .await
        .expect("Failed to connect to Wi-Fi");
    esp_println::println!("Connected; waiting for DHCP");

    let (stack, runner) = embassy_net::new(
        interfaces.sta,
        embassy_net::Config::dhcpv4(Default::default()),
        NETWORK_RESOURCES.take(),
        0xC6FE_A7E2,
    );
    spawner
        .spawn(network_task(runner))
        .expect("Failed to spawn the network task");
    stack.wait_config_up().await;
    let address = stack
        .config_v4()
        .expect("Network stack has no IPv4 configuration")
        .address
        .address();
    esp_println::println!("Servo control page: http://{address}/");
    spawner
        .spawn(http_task(stack))
        .expect("Failed to spawn the HTTP server");

    let mut servo_tick = 0;
    loop {
        let angle = u32::from(SERVO_TARGET_DEGREES.load(Ordering::Relaxed));
        let pulse_width_us = 1_000 + angle * 1_000 / 180;
        let duty = (pulse_width_us * u32::from(servo.max_duty_cycle()) / 20_000) as u16;
        servo
            .set_duty_cycle(duty)
            .expect("Failed to update the servo position");

        servo_tick += 1;
        if servo_tick == 250 {
            let mut soc_bytes = [0; 2];
            i2c.write_read(0x36, &[0x04], &mut soc_bytes)
                .expect("Failed to read battery state of charge from MAX17048");
            let state_of_charge = (u16::from_be_bytes(soc_bytes) >> 8).min(100) as u8;
            let color = hsv2rgb(Hsv {
                hue: (u16::from(state_of_charge) * 85 / 100) as u8,
                sat: u8::MAX,
                val: u8::MAX,
            });
            led.write(brightness(gamma([color].into_iter()), 32))
                .expect("Failed to update the NeoPixel");
            servo_tick = 0;
        }

        Timer::after(Duration::from_millis(20)).await;
    }
}

fn configure_wifi(wifi: &mut esp_radio::wifi::WifiController<'_>) {
    wifi.set_config(&esp_radio::wifi::ModeConfig::Client(
        esp_radio::wifi::ClientConfig::default()
            .with_ssid(String::from(env!("WIFI_SSID")))
            .with_password(String::from(env!("WIFI_PASSWORD"))),
    ))
    .expect("Failed to configure Wi-Fi");
}

#[embassy_executor::task]
async fn network_task(
    mut runner: embassy_net::Runner<'static, esp_radio::wifi::WifiDevice<'static>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn http_task(stack: embassy_net::Stack<'static>) -> ! {
    let rx_buffer = HTTP_RX_BUFFER.take();
    let tx_buffer = HTTP_TX_BUFFER.take();
    let request_buffer = HTTP_REQUEST_BUFFER.take();

    loop {
        let mut socket = embassy_net::tcp::TcpSocket::new(stack, &mut *rx_buffer, &mut *tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(5)));
        socket
            .accept(80)
            .await
            .expect("Failed to accept an HTTP connection");

        if let Err(error) = handle_http_request(&mut socket, &mut *request_buffer).await {
            esp_println::println!("HTTP request failed: {error:?}");
            socket.abort();
        }
    }
}

async fn handle_http_request(
    socket: &mut embassy_net::tcp::TcpSocket<'_>,
    request: &mut [u8; 512],
) -> Result<(), embassy_net::tcp::Error> {
    let mut request_len = 0;

    loop {
        if request_len == request.len() {
            write_all(socket, BAD_REQUEST_RESPONSE).await?;
            break;
        }

        let bytes_read = socket.read(&mut request[request_len..]).await?;
        if bytes_read == 0 {
            write_all(socket, BAD_REQUEST_RESPONSE).await?;
            break;
        }
        request_len += bytes_read;

        if request[..request_len]
            .windows(4)
            .any(|window| window == b"\r\n\r\n")
        {
            let request_line = request[..request_len]
                .split(|byte| *byte == b'\n')
                .next()
                .unwrap_or_default();

            if request_line.starts_with(b"GET /servo?angle=") {
                if let Some(angle) = parse_servo_angle(request_line) {
                    SERVO_TARGET_DEGREES.store(angle, Ordering::Relaxed);
                    write_all(socket, NO_CONTENT_RESPONSE).await?;
                } else {
                    write_all(socket, BAD_REQUEST_RESPONSE).await?;
                }
            } else if request_line.starts_with(b"GET / HTTP/") {
                write_all(socket, INDEX_RESPONSE).await?;
            } else {
                write_all(socket, NOT_FOUND_RESPONSE).await?;
            }
            break;
        }
    }

    socket.close();
    socket.flush().await
}

fn parse_servo_angle(request_line: &[u8]) -> Option<u8> {
    let target = request_line
        .strip_prefix(b"GET ")?
        .split(|byte| *byte == b' ')
        .next()?;
    let digits = target.strip_prefix(b"/servo?angle=")?;
    if digits.is_empty() {
        return None;
    }

    let angle = digits.iter().try_fold(0_u16, |angle, digit| {
        if digit.is_ascii_digit() {
            angle.checked_mul(10)?.checked_add(u16::from(digit - b'0'))
        } else {
            None
        }
    })?;
    (angle <= 180).then_some(angle as u8)
}

async fn write_all(
    socket: &mut embassy_net::tcp::TcpSocket<'_>,
    mut bytes: &[u8],
) -> Result<(), embassy_net::tcp::Error> {
    while !bytes.is_empty() {
        let written = socket.write(bytes).await?;
        if written == 0 {
            return Err(embassy_net::tcp::Error::ConnectionReset);
        }
        bytes = &bytes[written..];
    }
    Ok(())
}
