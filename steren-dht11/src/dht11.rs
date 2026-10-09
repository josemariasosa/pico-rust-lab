use super::*;

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
enum HandshakeError {
    NoInitialLow,
    LowDidNotEnd,
    LowInvalidDuration,
    HighDidNotEnd,
    HighInvalidDuration,
}

pub(crate) struct Dht11 {
    pin: Option<DhtInputPin>,
}

impl Dht11 {
    pub(crate) fn new(pin: DhtInputPin) -> Self {
        Self { pin: Some(pin) }
    }

    pub(crate) fn read(
        &mut self,
        timer: &hal::Timer<hal::timer::CopyableTimer0>,
    ) -> Result<(), HandshakeError> {
        let pin = self.pin.take().unwrap();

        let (pin, result) = Self::start_transaction(timer, pin);

        self.pin = Some(pin);

        result
    }

    fn start_transaction(
        timer: &hal::Timer<hal::timer::CopyableTimer0>,
        pin: DhtInputPin,
    ) -> (DhtInputPin, Result<(), HandshakeError>) {
        // 1. Input -> Output.
        let mut pin = pin.into_push_pull_output();

        // 2. Start signal.
        pin.set_low().unwrap();

        let start = timer.get_counter();
        while (timer.get_counter() - start).to_millis() < 18 {}

        // 3. Release DATA.
        let mut pin = pin.into_floating_input();

        // 4. Validate handshake without consuming the pin.
        let result = Self::validate_response(timer, &mut pin);

        (pin, result)
    }

    fn validate_response(
        timer: &hal::Timer<hal::timer::CopyableTimer0>,
        pin: &mut DhtInputPin,
    ) -> Result<(), HandshakeError> {
        // 1. Wait for the initial LOW response.
        let start = timer.get_counter();

        loop {
            if pin.is_low().unwrap() {
                break;
            }

            if (timer.get_counter() - start).to_micros() >= 120 {
                return Err(HandshakeError::NoInitialLow);
            }
        }

        // 2. Measure the LOW response pulse.
        let start = timer.get_counter();

        let low_us = loop {
            if pin.is_high().unwrap() {
                break (timer.get_counter() - start).to_micros();
            }

            if (timer.get_counter() - start).to_micros() >= 150 {
                return Err(HandshakeError::LowDidNotEnd);
            }
        };

        if !(50..=110).contains(&low_us) {
            return Err(HandshakeError::LowInvalidDuration);
        }

        // 3. Measure the HIGH response pulse.
        let start = timer.get_counter();

        let high_us = loop {
            if pin.is_low().unwrap() {
                break (timer.get_counter() - start).to_micros();
            }

            if (timer.get_counter() - start).to_micros() >= 150 {
                return Err(HandshakeError::HighDidNotEnd);
            }
        };

        if !(50..=110).contains(&high_us) {
            return Err(HandshakeError::HighInvalidDuration);
        }

        Ok(())
    }
}
