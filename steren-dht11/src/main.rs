#![no_std]
#![no_main]

// We need to create our own panic handler because we are in a no_std environment.
use core::panic::PanicInfo;

// Alias our HAL.
use rp235x_hal as hal;

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

enum HandshakeResult {
    Ok,
    NoInitialLow,
    LowDidNotEnd,
    HighDidNotEnd,
}

fn dht_handshake(
    timer: hal::Timer<hal::timer::CopyableTimer0>,
    mut dht_pin: hal::gpio::Pin<
        hal::gpio::bank0::Gpio15,
        hal::gpio::FunctionSio<hal::gpio::SioOutput>,
        hal::gpio::PullDown,
    >,
) -> HandshakeResult {
    dht_pin.set_low().unwrap();

    let start = timer.get_counter();
    while (timer.get_counter() - start).to_millis() < 18 {}

    // dht_pin.set_high().unwrap();

    // let start = timer.get_counter();
    // while (timer.get_counter() - start).to_micros() < 30 {}

    // Release the bus immediately.
    // External 10k pull-up should bring DATA high.

    let mut dht_pin = dht_pin.into_floating_input();

    let start = timer.get_counter();
    while dht_pin.is_high().unwrap() {
        if (timer.get_counter() - start).to_micros() > 100 {
            return HandshakeResult::NoInitialLow;
        }
    }

    // We already detected that the DHT11 pulled DATA low.

    // 1. Wait for the response LOW pulse to finish.
    //    LOW -> HIGH
    let start = timer.get_counter();
    while dht_pin.is_low().unwrap() {
        if (timer.get_counter() - start).to_micros() > 120 {
            return HandshakeResult::LowDidNotEnd;
        }
    }

    // 2. Wait for the response HIGH pulse to finish.
    //    HIGH -> LOW
    let start = timer.get_counter();
    while dht_pin.is_high().unwrap() {
        if (timer.get_counter() - start).to_micros() > 120 {
            return HandshakeResult::HighDidNotEnd;
        }
    }

    HandshakeResult::Ok
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

    // Give the DHT11 time to stabilize after power-on.
    let start = timer.get_counter();
    while (timer.get_counter() - start).to_millis() < 2_000 {}

    // Start signal.
    let mut dht_pin = pins.gpio15.into_push_pull_output();

    dht_pin.set_low().unwrap();

    let start = timer.get_counter();
    while (timer.get_counter() - start).to_millis() < 18 {}

    // Release DATA.
    let mut dht_pin = dht_pin.into_floating_input();

    // Diagnostic timeout intentionally much longer than protocol timing.
    let start = timer.get_counter();

    let responded = loop {
        if dht_pin.is_low().unwrap() {
            break true;
        }

        if (timer.get_counter() - start).to_micros() > 1_000 {
            break false;
        }
    };

    if responded {
        green_led.set_high().unwrap();
        red_led.set_low().unwrap();
    } else {
        red_led.set_high().unwrap();
        green_led.set_low().unwrap();
    }

    loop {}

    // let dht_pin = pins.gpio15.into_push_pull_output();
    // let handshake_result = dht_handshake(timer, dht_pin);

    // loop {
    //     match handshake_result {
    //         HandshakeResult::Ok => {
    //             red_led.set_low().unwrap();

    //             green_led.set_high().unwrap();

    //             let start = timer.get_counter();
    //             while (timer.get_counter() - start).to_millis() < 250 {}

    //             green_led.set_low().unwrap();

    //             let start = timer.get_counter();
    //             while (timer.get_counter() - start).to_millis() < 250 {}
    //         }

    //         HandshakeResult::NoInitialLow => {
    //             green_led.set_low().unwrap();

    //             // 1 blink
    //             red_led.set_high().unwrap();

    //             let start = timer.get_counter();
    //             while (timer.get_counter() - start).to_millis() < 150 {}

    //             red_led.set_low().unwrap();

    //             let start = timer.get_counter();
    //             while (timer.get_counter() - start).to_millis() < 800 {}
    //         }

    //         HandshakeResult::LowDidNotEnd => {
    //             green_led.set_low().unwrap();

    //             // 2 blinks
    //             for _ in 0..2 {
    //                 red_led.set_high().unwrap();

    //                 let start = timer.get_counter();
    //                 while (timer.get_counter() - start).to_millis() < 150 {}

    //                 red_led.set_low().unwrap();

    //                 let start = timer.get_counter();
    //                 while (timer.get_counter() - start).to_millis() < 150 {}
    //             }

    //             // Pause between groups
    //             let start = timer.get_counter();
    //             while (timer.get_counter() - start).to_millis() < 700 {}
    //         }

    //         HandshakeResult::HighDidNotEnd => {
    //             green_led.set_low().unwrap();

    //             // 3 blinks
    //             for _ in 0..3 {
    //                 red_led.set_high().unwrap();

    //                 let start = timer.get_counter();
    //                 while (timer.get_counter() - start).to_millis() < 150 {}

    //                 red_led.set_low().unwrap();

    //                 let start = timer.get_counter();
    //                 while (timer.get_counter() - start).to_millis() < 150 {}
    //             }

    //             // Pause between groups
    //             let start = timer.get_counter();
    //             while (timer.get_counter() - start).to_millis() < 700 {}
    //         }
    //     }
    // }

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
