#![no_std]
#![no_main]

// We need to create our own panic handler because we are in a no_std environment.
use core::panic::PanicInfo;

// Alias our HAL.
use rp235x_hal as hal;

mod dht11;
use dht11::Dht11;

// Import traits for embedded abstractions.
use embedded_hal::digital::OutputPin;

// USB device and communications class device (CDC) support
use usb_device::{class_prelude::*, prelude::*};
use usbd_serial::{SerialPort, USB_CLASS_CDC};

use core::fmt::Write;
use heapless::String;

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

    let mut dht11: Dht11 = Dht11::new(pins.gpio15.into_floating_input());

    // ---- USB SETUP ----
    // Initialize the USB driver
    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USB,
        pac.USB_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    // Configure the USB as CDC
    let mut serial = SerialPort::new(&usb_bus);

    // Create a USB device with fake VID and PID
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x16c0, 0x27dd))
        .strings(&[StringDescriptors::default()
            .manufacturer("Fake Manufacturer")
            .product("Serial Port")
            .serial_number("12345678")])
        .unwrap()
        .device_class(USB_CLASS_CDC) // from: https://www.usb.org/defined-class-codes
        .build();

    let mut last_read = timer.get_counter();
    let mut buffer: String<128> = String::new();
    let mut tx_offset = 0usize;
    let mut tx_pending = false;
    loop {
        // Keep USB communication alive.
        usb_dev.poll(&mut [&mut serial]);

        // Flush pending USB output without blocking.
        if tx_pending {
            match serial.write(&buffer.as_bytes()[tx_offset..]) {
                Ok(n) if n > 0 => {
                    tx_offset += n;

                    if tx_offset == buffer.len() {
                        tx_pending = false;
                    }
                }
                Ok(_) | Err(usb_device::UsbError::WouldBlock) => {}
                Err(_) => {
                    tx_pending = false;
                }
            }
        }

        // Run a new DHT11 transaction every 2 seconds.
        if (timer.get_counter() - last_read).to_millis() >= 2_000 {
            last_read = timer.get_counter();

            match dht11.read(&timer) {
                Ok(measurement) => {
                    green_led.set_high().unwrap();
                    red_led.set_low().unwrap();

                    // Do not overwrite a message still being sent.
                    if !tx_pending {
                        buffer.clear();

                        write!(
                            &mut buffer,
                            "[DHT11] Temperature: {} C | Humidity: {}%\r\n",
                            measurement.temperature_integer, measurement.humidity_integer,
                        )
                        .unwrap();

                        tx_offset = 0;
                        tx_pending = true;
                    }
                }

                Err(_) => {
                    red_led.set_high().unwrap();
                    green_led.set_low().unwrap();
                }
            }
        }
    }
}
