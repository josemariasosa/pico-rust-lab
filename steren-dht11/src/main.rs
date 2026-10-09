#![no_std]
#![no_main]

// We need to create our own panic handler because we are in a no_std environment.
use core::panic::PanicInfo;

// Alias our HAL.
use rp235x_hal as hal;

mod dht11;
use dht11::{Dht11, DhtInputPin, DhtOutputPin};

// Import traits for embedded abstractions.
// use embedded_hal::delay::DelayNs;
use embedded_hal::digital::InputPin;
use embedded_hal::digital::OutputPin;
// use embedded_hal::digital::StatefulOutputPin;

// USB device and communications class device (CDC) support
// use usb_device::{class_prelude::*, prelude::*};
// use usbd_serial::SerialPort;

// Panic handler just loops indefinitely.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

// Copy boot metadata to .start_block so boot ROM knows how to start the program.
#[unsafe(link_section = ".start_block")]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

// Set external crystal frequency for the system clock.
const XOSC_CRYSTAL_FREQ: u32 = 12_000_000; // 12 MHz external crystal frequency

fn dht_handshake(
    timer: &hal::Timer<hal::timer::CopyableTimer0>,
    mut dht_pin: DhtOutputPin,
) -> Result<DhtInputPin, HandshakeError> {
    // 1. MCU sends the start signal.
    dht_pin.set_low().unwrap();

    let start = timer.get_counter();
    while (timer.get_counter() - start).to_millis() < 18 {}

    // 2. Release DATA. External pull-up restores HIGH.
    let mut dht_pin = dht_pin.into_floating_input();

    // 3. Wait for the initial LOW response.
    let start = timer.get_counter();

    loop {
        if dht_pin.is_low().unwrap() {
            break;
        }

        if (timer.get_counter() - start).to_micros() >= 120 {
            return Err(HandshakeError::NoInitialLow);
        }
    }

    // 4. Measure the LOW response pulse.
    let start = timer.get_counter();

    let low_us = loop {
        if dht_pin.is_high().unwrap() {
            break (timer.get_counter() - start).to_micros();
        }

        if (timer.get_counter() - start).to_micros() >= 150 {
            return Err(HandshakeError::LowDidNotEnd);
        }
    };

    if !(50..=110).contains(&low_us) {
        return Err(HandshakeError::LowInvalidDuration);
    }

    // 5. Measure the HIGH response pulse.
    let start = timer.get_counter();

    let high_us = loop {
        if dht_pin.is_low().unwrap() {
            break (timer.get_counter() - start).to_micros();
        }

        if (timer.get_counter() - start).to_micros() >= 150 {
            return Err(HandshakeError::HighDidNotEnd);
        }
    };

    if !(50..=110).contains(&high_us) {
        return Err(HandshakeError::HighInvalidDuration);
    }

    Ok(dht_pin)
}

// Main entry point for the program.
#[hal::entry]
fn main() -> ! {
    // Get access of hardware peripherals.
    let mut pac = hal::pac::Peripherals::take().unwrap();

    // Set up watchdog and clocks.
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);

    let clocks = hal::clocks::init_clocks_and_plls(
        XOSC_CRYSTAL_FREQ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    // Move ownership of TIMER0 peripheral to create Timer struct.
    let timer: rp235x_hal::Timer<rp235x_hal::timer::CopyableTimer0> =
        hal::Timer::new_timer0(pac.TIMER0, &mut pac.RESETS, &clocks);

    // ---- PIN SETUP ----
    // Single-cycle I/O block (fast GPIO)
    let sio = hal::Sio::new(pac.SIO);

    // Split off ownership of peripherals struct, set pins to their initial states.
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // ---- LED SETUP ----
    // Configure pin, get ownership of the LED pin.
    let mut red_led = pins.gpio14.into_push_pull_output();
    let mut green_led = pins.gpio16.into_push_pull_output();

    // ---- DHT11 SETUP ----

    // Allow the sensor to stabilize after power-on.
    let start = timer.get_counter();
    while (timer.get_counter() - start).to_millis() < 2_000 {}

    let _dht_pin: Dht11 = Dht11::new(pins.gpio15.into_floating_input());

    // let result = dht_handshake(&timer, dht_pin);

    // if result.is_ok() {
    //     green_led.set_high().unwrap();
    //     red_led.set_low().unwrap();
    // } else {
    //     red_led.set_high().unwrap();
    //     green_led.set_low().unwrap();
    // }

    loop {}

    // // ---- USB SETUP ----
    // // Initialize the USB driver
    // let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
    //     pac.USB,
    //     pac.USB_DPRAM,
    //     clocks.usb_clock,
    //     true,
    //     &mut pac.RESETS,
    // ));

    // // Configure the USB as CDC
    // let mut serial = SerialPort::new(&usb_bus);

    // // Create a USB device with fake VID and PID
    // let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x16c0, 0x27dd))
    //     .strings(&[StringDescriptors::default()
    //         .manufacturer("Fake Manufacturer")
    //         .product("Serial Port")
    //         .serial_number("12345678")])
    //     .unwrap()
    //     .device_class(2) // from: https://www.usb.org/defined-class-codes
    //     .build();

    // // Read buffer
    // let mut rx_buf = [0u8; 64];

    // // Superloop
    // let mut timestamp = timer.get_counter();
    // let mut led_state = false;
    // loop {
    //     if usb_dev.poll(&mut [&mut serial]) {
    //         match serial.read(&mut rx_buf) {
    //             Ok(0) => {}
    //             Ok(_count) => {}
    //             Err(_e) => {}
    //         }
    //     }

    //     // Send message every second (non-blocking)
    //     if (timer.get_counter() - timestamp).to_millis() >= 1_000 {
    //         timestamp = timer.get_counter();
    //         let _ = serial.write(b"Hello, world!\r\n");

    //         // Toggle LED state every second
    //         led_state = !led_state;
    //         if led_state {
    //             red_led.set_high().unwrap();
    //         } else {
    //             red_led.set_low().unwrap();
    //         }
    //     }
    // }
}
