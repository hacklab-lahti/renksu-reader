use heapless::{consts::*, spsc::Queue};

use embedded_hal::PwmPin;

use stm32f1xx_hal::{
    afio::MAPR,
    pac,
    prelude::*,
    pwm::{Pins, PwmExt},
    rcc::{Clocks, APB1},
};

#[derive(Debug)]
pub struct Note {
    pub freq: u16,
    pub len: u8,
    pub duty: u8,
}

pub struct Player<TIM, PINS>
where
    TIM: PwmExt + TimerExt,
    PINS: Pins<TIM>,
{
    clocks: Clocks,
    pwm: PINS::Channels,
    notes: Queue<Note, U256>,
    ticks: u8,
}

impl<TIM, PINS> Player<TIM, PINS>
where
    TIM: PwmExt + TimerExt,
    PINS: Pins<TIM>,
    PINS::Channels: PwmPin<Duty = u16>,
{
    pub fn new(clocks: Clocks, tim: TIM, pins: PINS, mapr: &mut MAPR, apb1: &mut APB1) -> Self {
        let mut pwm = tim.pwm(pins, mapr, 1000.hz(), clocks, apb1);

        pwm.enable();
        pwm.set_duty(pwm.get_max_duty());

        Player {
            clocks,
            pwm,
            notes: Queue::new(),
            ticks: 0,
        }
    }

    pub fn stop(&mut self) {
        while !self.notes.is_empty() {
            self.notes.dequeue();
        }

        self.ticks = 0;
    }

    pub fn play(&mut self, note: Note) {
        self.notes.enqueue(note).ok();
    }

    pub fn tick(&mut self) {
        if self.ticks == 0 {
            while let Some(note) = self.notes.dequeue() {
                if note.len == 0 {
                    continue;
                }

                if note.freq == 0 {
                    self.pwm.disable()
                } else {
                    self.set_pwm_freq(note.freq as u32);
                    self.pwm.set_duty(
                        ((note.duty as u32 * self.pwm.get_max_duty() as u32) / 256) as u16,
                    );
                    self.pwm.enable();
                }

                self.ticks = note.len - 1;

                return;
            }

            self.pwm.disable();
        } else {
            self.ticks -= 1;
        }
    }

    fn set_pwm_freq(&mut self, freq: u32) {
        let clk = self.clocks.pclk1_tim().0;
        let ticks = clk / freq.hz().0;

        let psc = (ticks / (1 << 16)) as u16;
        let arr = (ticks / ((psc as u32) + 1)) as u16;

        TIM::set_psc_arr(psc, arr);
    }
}

pub trait TimerExt {
    fn set_psc_arr(psc: u16, arr: u16);
}

impl TimerExt for pac::TIM2 {
    fn set_psc_arr(psc: u16, arr: u16) {
        let tim = unsafe { &*pac::TIM2::ptr() };

        tim.psc.write(|w| unsafe { w.psc().bits(psc) });
        tim.arr.write(|w| w.arr().bits(arr));
    }
}
