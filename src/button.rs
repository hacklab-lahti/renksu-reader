use embedded_hal::digital::InputPin;

/// Simple debounced button
pub struct Button<PIN> {
    pin: PIN,
    debounce: usize,
    state: bool,
    count: usize,
}

impl<PIN> Button<PIN>
where PIN: InputPin
{
    pub fn new(pin: PIN, debounce: usize) -> Self {
        Button {
            pin,
            debounce,
            state: false,
            count: 0,
        }
    }

    pub fn poll(&mut self) -> Option<bool> {
        if self.pin.is_low() {
            self.count = self.debounce;

            if !self.state {
                self.state = true;
                return Some(true);
            }
        } else if self.state && self.count > 0 {
            self.count -= 1;

            if self.count == 0 {
                self.state = false;
                return Some(false);
            }
        }

        None
    }
}