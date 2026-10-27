#![no_std]
#![no_main]

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
use esp_hal_smartled::{SmartLedsAdapter, smart_led_buffer};
use smart_leds::{
    SmartLedsWrite, brightness, gamma,
    hsv::{Hsv, hsv2rgb},
};

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

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
    let mut rmt_buffer = smart_led_buffer!(1);
    let mut led = SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO9, &mut rmt_buffer);

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

    loop {
        for pulse_width_us in (1_000_u32..=1_500).step_by(50) {
            let duty = (pulse_width_us * u32::from(servo.max_duty_cycle()) / 20_000) as u16;
            servo
                .set_duty_cycle(duty)
                .expect("Failed to update the servo position");
            Timer::after(Duration::from_millis(100)).await;
        }

        for pulse_width_us in (1_000_u32..=1_500).rev().step_by(50) {
            let duty = (pulse_width_us * u32::from(servo.max_duty_cycle()) / 20_000) as u16;
            servo
                .set_duty_cycle(duty)
                .expect("Failed to update the servo position");
            Timer::after(Duration::from_millis(100)).await;
        }

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
    }
}
