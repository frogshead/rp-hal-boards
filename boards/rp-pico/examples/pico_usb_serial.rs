// q: What does this code does?

//! # Pico USB Serial Example
//!
//! Creates a USB Serial device on a Pico board, with the USB driver running in
//! the main thread.
//!
//! This will create a USB Serial device echoing anything it receives. Incoming
//! ASCII characters are converted to upercase, so you can tell it is working
//! and not just local-echo!
//!
//! See the `Cargo.toml` file for Copyright and license details.

#![no_std]
#![no_main]

use embedded_hal::adc::OneShot;
use embedded_hal::digital::v2::OutputPin;
use hal::gpio::PinState;
use onewire::{ DeviceSearch, OneWire};
use rp_pico::hal::Clock;
// The macro for our start-up function
use rp_pico::entry;

// Ensure we halt the program on panic (if we don't mention this crate it won't
// be linked)
use panic_halt as _;

// A shorter alias for the Peripheral Access Crate, which provides low-level
// register access
use rp_pico::hal::pac;

// A shorter alias for the Hardware Abstraction Layer, which provides
// higher-level drivers.
use rp_pico::hal;

// USB Device support
use usb_device::{class_prelude::*, prelude::*};

// USB Communications Class Device support
use usbd_serial::SerialPort;

// Used to demonstrate writing formatted strings
use core::fmt::Write;
use heapless::String;

/// Entry point to our bare-metal application.
///
/// The `#[entry]` macro ensures the Cortex-M start-up code calls this function
/// as soon as all global variables are initialised.
///
/// The function configures the RP2040 peripherals, then echoes any characters
/// received over USB Serial.
#[entry]
fn main() -> ! {
    // Grab our singleton objects
    let mut pac = pac::Peripherals::take().unwrap();

    // Set up the watchdog driver - needed by the clock setup code
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);

    // Configure the clocks
    //
    // The default is to generate a 125 MHz system clock
    let clocks = hal::clocks::init_clocks_and_plls(
        rp_pico::XOSC_CRYSTAL_FREQ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    // The single-cycle I/O block controls our GPIO pins
    let sio = hal::Sio::new(pac.SIO);

    // Set the pins up according to their function on this particular board
    let pins = rp_pico::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // The delay object lets us wait for specified amounts of time (in
    // milliseconds)
    let core = pac::CorePeripherals::take().unwrap();
    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());

    // Set the LED to be an output
    let mut led_pin = pins.gpio26.into_push_pull_output();
    led_pin.set_low().unwrap();

    let mut one = pins.gpio16.into_readable_output();
    let mut wire = OneWire::new(&mut one, false);

    #[cfg(feature = "rp2040-e5")]
    {
        let sio = hal::Sio::new(pac.SIO);
        let _pins = rp_pico::Pins::new(
            pac.IO_BANK0,
            pac.PADS_BANK0,
            sio.gpio_bank0,
            &mut pac.RESETS,
        );
    }

    // Set up the USB driver
    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    // Set up the USB Communications Class Device driver
    let mut serial = SerialPort::new(&usb_bus);

    // Create a USB device with a fake VID and PID
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x16c0, 0x27dd))
        .manufacturer("Fake company")
        .product("Serial port")
        .serial_number("TEST")
        .device_class(2) // from: https://www.usb.org/defined-class-codes
        .build();

    let timer = hal::Timer::new(pac.TIMER, &mut pac.RESETS);

    match wire.reset(&mut delay){
        Err(_) => {
            loop {
                let _ = serial.write(b"Reset failed\r\n");
            }
        }
        Ok(pulled_up) => {
            
                let mut text: String<64> = String::new();
                writeln!(&mut text, "Reset succeeded. Pulled up: {}", pulled_up).unwrap();
                let _ = serial.write(text.as_bytes());
            
        }
    }

    let mut search = DeviceSearch::new();

    let mut said_hello = false;
    let mut pin_state = PinState::from(false);
    let mut adc = hal::adc::Adc::new(pac.ADC, &mut pac.RESETS);
    let mut adc_pin = pins.gpio27.into_floating_input();
    let mut reading: u16 = adc.read(&mut adc_pin).unwrap();
    loop {
        // A welcome message to show we're alive
        if !said_hello && timer.get_counter().ticks() >= 5_000_000 {
            said_hello = true;

            let _ = serial.write(b"Hello, world!\r\n");

            let time = timer.get_counter().ticks();
            let mut text: String<64> = String::new();
            writeln!(&mut text, "Current timer ticks: {}\r\n", time).unwrap();
            // writeln!(&mut text, "Temp reading: {}\r\n", internal_temp_sensor).unwrap();
            writeln!(&mut text, "ADC reading: {}\r\n", reading).unwrap();

            // This only works reliably because the number of bytes written to
            // the serial port is smaller than the buffers available to the USB
            // peripheral. In general, the return value should be handled, so that
            // bytes not transferred yet don't get lost.
            let _ = serial.write(text.as_bytes());

            let _ = serial.write(b"Searching for devices...\r\n");
            let next = wire.search_next(&mut search, &mut delay).unwrap();
            match next {
                Some(device) => {
                    let mut text: String<64> = String::new();
                    let _ = serial.write(b"Device found\r\n");
                    let r = writeln!(
                        &mut text,
                        "\r\nFound device with family code: 0x{:x}\r\n",
                        device.family_code()
                    );

                    let mut text: String<64> = String::new();
                    let _ = write!(&mut text, " Device found :{}", device);

                    match r {
                        Ok(_) => {
                            let _ = serial.write(b"\r\nOK!");
                        }
                        Err(_) => {
                            let _x = serial.write(b"Something odd happened");
                        }
                    }
                    
                    // unsafe{
                    //     // let mut ds = DS18B20::new_forced(device);
                    //     // let res = ds.measure_temperature(&mut wire, &mut delay).unwrap();
                    //     // // let temp = ds.read_temperature(&mut wire, &mut delay).unwrap();
                    //     // let mut text: String<64> = String::new();
                    //     // let _ = writeln!(&mut text, "Temperature: {} C", temp);
                    //     // let _ = serial.write(text.as_bytes());

                    // }

                    
                   // let resolution = temp_sensor.measure_temperature(&mut wire, &mut delay);
                }
                None => {
                    let _ = serial.write(b"No Devices found...\r\n");
                }
            }
        }
        // Check for new data
        if usb_dev.poll(&mut [&mut serial]) {
            let mut buf = [0u8; 64];
            match serial.read(&mut buf) {
                Err(_e) => {
                    // Do nothing
                }
                Ok(0) => {

                    // Do nothing
                }
                Ok(count) => {
                    // Convert to upper case
                    buf.iter_mut().take(count).for_each(|b| {
                        b.make_ascii_uppercase();
                    });
                    // Send back to the host
                    let mut wr_ptr = &buf[..count];
                    while !wr_ptr.is_empty() {
                        match serial.write(wr_ptr) {
                            Ok(len) => {
                                wr_ptr = &wr_ptr[len..];
                                pin_state = !pin_state;
                                led_pin.set_state(pin_state).unwrap();
                            }
                            // On error, just drop unwritten data.
                            // One possible error is Err(WouldBlock), meaning the USB
                            // write buffer is full.
                            Err(_) => break,
                        };
                    }
                }
            }
        }
        if said_hello == true {
            // let mut t: String<64> = String::new();
            reading = adc.read(&mut adc_pin).unwrap();
            // let _ = serial.write("".as_bytes());
            // write!(&mut t, "ADC1 reading: {}\r", reading).unwrap();
            // let _ = serial.write(t.as_bytes());
        }
    }
}

// End of file
