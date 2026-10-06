#![no_std]
#![no_main]

// We need to create our own panic handler because we are in a no_std environment.
use core::panic::PanicInfo;

// Alias our HAL.
use rp235x_hal as hal;

// USB device and communications class device (CDC) support
use usb_device::{class_prelude::*, prelude::*};
use usbd_serial::SerialPort;

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
    let timer = hal::Timer::new_timer0(pac.TIMER0, &mut pac.RESETS, &clocks);

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
        .device_class(2) // from: https://www.usb.org/defined-class-codes
        .build();

    // Read buffer
    let mut rx_buf = [0u8; 64];

    // Superloop
    let mut timestamp = timer.get_counter();
    loop {
        if usb_dev.poll(&mut [&mut serial]) {
            match serial.read(&mut rx_buf) {
                Ok(0) => {}
                Ok(_count) => {}
                Err(_e) => {}
            }
        }

        // Send message every second (non-blocking)
        if (timer.get_counter() - timestamp).to_millis() >= 1_000 {
            timestamp = timer.get_counter();
            let _ = serial.write(b"Hello, world!\r\n");
        }
    }
}
