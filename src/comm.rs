use crate::player::Note;
use core::convert::TryInto;
use embedded_hal::{
    digital::OutputPin,
    serial::{Read, Write},
};
use heapless::{consts::*, spsc::Queue, Vec};
use nb::block;

// Framing and escaping:
//
// '\n' = end of message
// '\\' 'n' = literal '\n'
// '\\' _ = literal _
//
// Command:
//
// 'P' = ping
// 'D' [1024 bytes of data] = display data
// 'L' [0 | 1] = set LED
// 'B' [(u16 freq, u8 length in units of 10ms, u8 volume)*] = beep
//
// Event:
//
// 'p' = pong (no event)
// 'b' [0 | 1] = button state
// 'r' [4-10 bytes of data] = RFID UID read

pub const BAUD_RATE: u32 = 115_200; // 115_200;

#[derive(Debug)]
pub enum Command<'a> {
    Ping,
    Reset,
    Display(&'a [u8]),
    Led(bool),
    Beep(Notes<'a>),
}

#[derive(Debug)]
pub struct Notes<'a>(&'a [u8]);

impl Iterator for Notes<'_> {
    type Item = Note;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.len() >= 4 {
            let note = Note {
                freq: u16::from_le_bytes(self.0[..2].try_into().unwrap()),
                len: self.0[2],
                duty: self.0[3],
            };

            self.0 = &self.0[4..];

            Some(note)
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub enum Event {
    Pong,
    Button(bool),
    Rfid(Vec<u8, U10>),
}

pub struct Comm<TX, RX, DE> {
    tx: TX,
    rx: RX,
    de: DE,
    events: Queue<Event, U4>,
    rx_buf: Vec<u8, U2048>,
    esc: bool,
}

trait ExtendEsc {
    fn extend_esc(&mut self, data: &[u8]) -> Result<(), u8>;
}

impl<N: heapless::ArrayLength<u8>> ExtendEsc for Vec<u8, N> {
    fn extend_esc(&mut self, data: &[u8]) -> Result<(), u8> {
        for b in data {
            match b {
                b'\n' => {
                    self.push(b'\\')?;
                    self.push(b'n')?;
                }
                b'\\' => {
                    self.push(b'\\')?;
                    self.push(b'\\')?;
                }
                b => {
                    self.push(*b)?;
                }
            }
        }

        Ok(())
    }
}

impl<TX, RX, DE> Comm<TX, RX, DE>
where
    TX: Write<u8>,
    RX: Read<u8>,
    DE: OutputPin,
{
    pub fn new(tx: TX, rx: RX, de: DE) -> Self {
        Comm {
            tx,
            rx,
            de,
            events: Queue::new(),
            rx_buf: Vec::new(),
            esc: false,
        }
    }

    pub fn send(&mut self, ev: Event) -> Result<(), ()> {
        self.events.enqueue(ev).map_err(|_| ())
    }

    pub fn clear_events(&mut self) {
        while self.events.dequeue().is_some() { }
    }

    pub fn handle_rx(&mut self) -> Option<Command> {
        let b = self.rx.read().ok()?;

        if self.esc {
            let b = match b {
                b'n' => b'\n',
                _ => b,
            };

            self.rx_buf.push(b).ok();
            self.esc = false;
        } else {
            match b {
                b'\n' => return self.complete_read(),
                b'\\' => {
                    self.esc = true;
                }
                b => {
                    self.rx_buf.push(b).ok();
                }
            };
        }

        None
    }

    fn complete_read(&mut self) -> Option<Command> {
        if self.rx_buf.len() == 0 {
            return None;
        }

        match self.rx_buf[0] {
            b'P' => Some(Command::Ping),
            b'R' => Some(Command::Reset),
            b'D' if self.rx_buf.len() == (1 + 1024) => Some(Command::Display(&self.rx_buf[1..])),
            b'L' if self.rx_buf.len() == 2 => Some(Command::Led(self.rx_buf[1] != 0)),
            b'B' => Some(Command::Beep(Notes(&self.rx_buf[1..]))),
            _ => {
                self.respond();
                None
            }
        }
    }

    pub fn respond(&mut self) {
        self.rx_buf.clear();

        let event = self.events.dequeue().unwrap_or(Event::Pong);

        let mut response = Vec::<u8, U16>::new();

        match event {
            Event::Pong => {
                response.push(b'p').unwrap();
            }
            Event::Button(pressed) => {
                response.push(b'b').unwrap();
                response.push(if pressed { 1 } else { 0 }).unwrap();
            }
            Event::Rfid(uid) => {
                response.push(b'r').unwrap();
                response.extend_esc(&uid).unwrap();
            }
        };

        response.push(b'\n').unwrap();

        // sleep for two bit times to make sure the host isn't driving the line anymore
        cortex_m::asm::delay(2 * 8_000_000 / BAUD_RATE);

        self.de.set_high();

        for b in response {
            block!(self.tx.write(b)).ok();
        }

        block!(self.tx.flush()).ok();

        // Dummy read to clear RX buffer
        block!(self.rx.read()).ok();

        self.de.set_low();
    }
}
