use rp235x_hal as hal;

use embedded_hal::digital::InputPin;
use embedded_hal::digital::OutputPin;

// pub(crate) type DhtOutputPin = hal::gpio::Pin<
//     hal::gpio::bank0::Gpio15,
//     hal::gpio::FunctionSio<hal::gpio::SioOutput>,
//     hal::gpio::PullDown,
// >;

pub(crate) type DhtInputPin = hal::gpio::Pin<
    hal::gpio::bank0::Gpio15,
    hal::gpio::FunctionSio<hal::gpio::SioInput>,
    hal::gpio::PullNone,
>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DhtError {
    // Handshake
    NoInitialLow,
    LowDidNotEnd,
    LowInvalidDuration,
    HighDidNotEnd,
    HighInvalidDuration,
    // Reading bits
    BitLowDidNotEnd,
    BitHighDidNotEnd,
    ChecksumMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Measurement {
    pub humidity_integer: u8,
    pub humidity_decimal: u8,
    pub temperature_integer: u8,
    pub temperature_decimal: u8,
}

impl From<[u8; 5]> for Measurement {
    fn from(frame: [u8; 5]) -> Self {
        Self {
            humidity_integer: frame[0],
            humidity_decimal: frame[1],
            temperature_integer: frame[2],
            temperature_decimal: frame[3],
        }
    }
}

pub(crate) struct Dht11 {
    pin: Option<DhtInputPin>,
}

impl Dht11 {
    pub(crate) fn new(pin: DhtInputPin) -> Self {
        Self { pin: Some(pin) }
    }

    // fn read_first_bit(
    //     timer: &hal::Timer<hal::timer::CopyableTimer0>,
    //     pin: &mut DhtInputPin,
    // ) -> Result<bool, HandshakeError> {
    //     // We enter while DATA is already LOW.
    //     // Wait for the beginning of the HIGH pulse.
    //     let start = timer.get_counter();

    //     loop {
    //         if pin.is_high().unwrap() {
    //             break;
    //         }

    //         if (timer.get_counter() - start).to_micros() >= 100 {
    //             return Err(HandshakeError::BitLowDidNotEnd);
    //         }
    //     }

    //     // Measure how long DATA remains HIGH.
    //     let start = timer.get_counter();

    //     let high_us = loop {
    //         if pin.is_low().unwrap() {
    //             break (timer.get_counter() - start).to_micros();
    //         }

    //         if (timer.get_counter() - start).to_micros() >= 100 {
    //             return Err(HandshakeError::BitHighDidNotEnd);
    //         }
    //     };

    //     // Short HIGH = 0, long HIGH = 1.
    //     Ok(high_us > 45)
    // }

    pub(crate) fn read(
        &mut self,
        timer: &hal::Timer<hal::timer::CopyableTimer0>,
    ) -> Result<Measurement, DhtError> {
        let pin = self.pin.take().unwrap();

        let (pin, result) = Self::start_transaction(timer, pin);

        self.pin = Some(pin);

        result
    }

    fn read_bit(
        timer: &hal::Timer<hal::timer::CopyableTimer0>,
        pin: &mut DhtInputPin,
    ) -> Result<bool, DhtError> {
        // DATA is already LOW.
        // Wait until the LOW pulse ends.
        let start = timer.get_counter();

        loop {
            if pin.is_high().unwrap() {
                break;
            }

            if (timer.get_counter() - start).to_micros() >= 100 {
                return Err(DhtError::BitLowDidNotEnd);
            }
        }

        // Measure the HIGH pulse.
        let start = timer.get_counter();

        let high_us = loop {
            if pin.is_low().unwrap() {
                break (timer.get_counter() - start).to_micros();
            }

            if (timer.get_counter() - start).to_micros() >= 100 {
                return Err(DhtError::BitHighDidNotEnd);
            }
        };

        // Short HIGH = 0, long HIGH = 1.
        Ok(high_us > 45)
    }

    fn read_byte(
        timer: &hal::Timer<hal::timer::CopyableTimer0>,
        pin: &mut DhtInputPin,
    ) -> Result<u8, DhtError> {
        let mut byte = 0u8;

        for _ in 0..8 {
            let bit = Self::read_bit(timer, pin)?;

            byte = (byte << 1) | u8::from(bit);
        }

        Ok(byte)
    }

    fn read_frame(
        timer: &hal::Timer<hal::timer::CopyableTimer0>,
        pin: &mut DhtInputPin,
    ) -> Result<[u8; 5], DhtError> {
        let mut frame = [0u8; 5];

        for byte in &mut frame {
            *byte = Self::read_byte(timer, pin)?;
        }

        Ok(frame)
    }

    fn validate_checksum(frame: &[u8; 5]) -> Result<(), DhtError> {
        let expected = frame[..4]
            .iter()
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte));

        if expected != frame[4] {
            return Err(DhtError::ChecksumMismatch);
        }

        Ok(())
    }

    fn start_transaction(
        timer: &hal::Timer<hal::timer::CopyableTimer0>,
        pin: DhtInputPin,
    ) -> (DhtInputPin, Result<Measurement, DhtError>) {
        // 1. Input -> Output.
        let mut pin = pin.into_push_pull_output();

        // 2. Start signal.
        pin.set_low().unwrap();

        let start = timer.get_counter();
        while (timer.get_counter() - start).to_millis() < 18 {}

        // 3. Release DATA.
        let mut pin = pin.into_floating_input();

        // 4. Execute the complete protocol.
        let result = (|| {
            Self::validate_response(timer, &mut pin)?;

            let frame = Self::read_frame(timer, &mut pin)?;

            Self::validate_checksum(&frame)?;

            Ok(Measurement::from(frame))
        })();

        (pin, result)
    }

    fn validate_response(
        timer: &hal::Timer<hal::timer::CopyableTimer0>,
        pin: &mut DhtInputPin,
    ) -> Result<(), DhtError> {
        // 1. Wait for the initial LOW response.
        let start = timer.get_counter();

        loop {
            if pin.is_low().unwrap() {
                break;
            }

            if (timer.get_counter() - start).to_micros() >= 120 {
                return Err(DhtError::NoInitialLow);
            }
        }

        // 2. Measure the LOW response pulse.
        let start = timer.get_counter();

        let low_us = loop {
            if pin.is_high().unwrap() {
                break (timer.get_counter() - start).to_micros();
            }

            if (timer.get_counter() - start).to_micros() >= 150 {
                return Err(DhtError::LowDidNotEnd);
            }
        };

        if !(50..=110).contains(&low_us) {
            return Err(DhtError::LowInvalidDuration);
        }

        // 3. Measure the HIGH response pulse.
        let start = timer.get_counter();

        let high_us = loop {
            if pin.is_low().unwrap() {
                break (timer.get_counter() - start).to_micros();
            }

            if (timer.get_counter() - start).to_micros() >= 150 {
                return Err(DhtError::HighDidNotEnd);
            }
        };

        if !(50..=110).contains(&high_us) {
            return Err(DhtError::HighInvalidDuration);
        }

        Ok(())
    }
}
