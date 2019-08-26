#![no_std]
#![no_main]
#![allow(deprecated)] // thanks embedded-hal

use heapless::{consts::*, Vec};
use panic_semihosting as _;
use rtfm::app;
use stm32f1xx_hal::{
    gpio::{gpioa::*, gpiob::*, Alternate, Floating, Input, Output, PullUp, PushPull},
    pac,
    prelude::*,
    serial::{self, Rx, Serial, Tx},
    spi::Spi,
};

#[allow(unused)]
#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {
        cortex_m::interrupt::free(|_| {
            let itm = unsafe { &mut *cortex_m::peripheral::ITM::ptr() };
            cortex_m::iprintln!(&mut itm.stim[0], $($arg)*);
        });
    }
}

mod button;
mod comm;
mod player;
mod sh1106_data_mode;
mod shared_spi;

use button::Button;
use comm::*;
use player::*;
use sh1106_data_mode::DataMode;
use shared_spi::SharedSpi;

const MS: u32 = 8_000_000 / 1000;

const COMM_ERROR_IMG: &'static [u8] = include_bytes!("../res/comm-error.img");

type Spi1 = SharedSpi<
    Spi<
        pac::SPI1,
        (
            PA5<Alternate<PushPull>>,
            PA6<Input<Floating>>,
            PA7<Alternate<PushPull>>,
        )>>;

#[app(device = stm32f1xx_hal::stm32)]
const APP: () = {
    static mut RFID: mfrc522::Mfrc522<
        mfrc522::interface::SpiInterface<&'static Spi1, PA3<Output<PushPull>>>,
    > = ();

    static mut DISP: DataMode<
        sh1106::interface::SpiInterface<&'static Spi1, PA2<Output<PushPull>>, PA1<Output<PushPull>>>,
    > = ();

    static mut BTN: Button<PA4<Input<PullUp>>> = ();

    static mut LED: PB0<Output<PushPull>> = ();

    static mut BEEPER: Player<pac::TIM2, PA0<Alternate<PushPull>>> = ();

    static mut COMM: Comm<Tx<pac::USART3>, Rx<pac::USART3>, PB1<Output<PushPull>>> = ();

    static mut COMMTIMEOUT: u32 = 0;

    static mut DISPBUF: Option<[u8; 1024]> = None;

    #[init(spawn = [tick])]
    fn init(c: init::Context) -> init::LateResources {
        static mut SPI1: Option<Spi1> = None;

        let mut rcc = c.device.RCC.constrain();
        let mut flash = c.device.FLASH.constrain();
        let clocks = rcc.cfgr.use_hse(8.mhz()).freeze(&mut flash.acr);

        cortex_m::asm::delay(10 * MS);

        let mut afio = c.device.AFIO.constrain(&mut rcc.apb2);
        let mut gpioa = c.device.GPIOA.split(&mut rcc.apb2);
        let mut gpiob = c.device.GPIOB.split(&mut rcc.apb2);

        let spi_sck_pin = gpioa.pa5.into_alternate_push_pull(&mut gpioa.crl);
        let spi_miso_pin = gpioa.pa6;
        let spi_mosi_pin = gpioa.pa7.into_alternate_push_pull(&mut gpioa.crl);
        let sh1106_dc_pin = gpioa.pa2.into_push_pull_output(&mut gpioa.crl);
        let mut mfrc522_nss_pin = gpioa.pa3.into_push_pull_output(&mut gpioa.crl);
        let sh1106_cs_pin = gpioa.pa1.into_push_pull_output(&mut gpioa.crl);
        let beeper_pin = gpioa.pa0.into_alternate_push_pull(&mut gpioa.crl);
        let serial_tx_pin = gpiob.pb10.into_alternate_push_pull(&mut gpiob.crh);
        let serial_rx_pin = gpiob.pb11;
        let mut serial_de_pin = gpiob.pb1.into_push_pull_output(&mut gpiob.crl);
        let btn_pin = gpioa.pa4.into_pull_up_input(&mut gpioa.crl);
        let mut led_pin = gpiob.pb0.into_push_pull_output(&mut gpiob.crl);

        mfrc522_nss_pin.set_high();
        serial_de_pin.set_low();
        led_pin.set_low();

        let mut serial = Serial::usart3(
            c.device.USART3,
            (serial_tx_pin, serial_rx_pin),
            &mut afio.mapr,
            BAUD_RATE.bps(),
            clocks,
            &mut rcc.apb1,
        );

        serial.listen(serial::Event::Rxne);

        let (tx, rx) = serial.split();

        let comm = Comm::new(tx, rx, serial_de_pin);

        *SPI1 = Some(SharedSpi::new(Spi::spi1(
            c.device.SPI1,
            (spi_sck_pin, spi_miso_pin, spi_mosi_pin),
            &mut afio.mapr,
            embedded_hal::spi::MODE_0,
            400.khz(),
            clocks,
            &mut rcc.apb2,
        )));

        let mut disp: DataMode<_> = sh1106::Builder::new()
            .with_spi_cs(sh1106_cs_pin)
            .connect_spi(SPI1.as_ref().unwrap(), sh1106_dc_pin)
            .into();

        let rfid =
            mfrc522::Mfrc522::new_spi(SPI1.as_ref().unwrap(), mfrc522_nss_pin).unwrap();

        let beeper = Player::new(
            clocks,
            c.device.TIM2,
            beeper_pin,
            &mut afio.mapr,
            &mut rcc.apb1,
        );

        let btn = Button::new(btn_pin, 10);

        disp.init().ok();
        disp.clear().ok();

        c.spawn.tick().unwrap();

        init::LateResources {
            RFID: rfid,
            DISP: disp,
            BTN: btn,
            LED: led_pin,
            BEEPER: beeper,
            COMM: comm,
        }
    }

    #[task(resources = [RFID, DISP, DISPBUF, COMM], priority = 1, capacity = 10)]
    fn spi(c: spi::Context, event: SpiEvent) {
        // Only this task may use the SPI peripherals

        let rfid = c.resources.RFID;
        let disp = c.resources.DISP;
        let mut disp_buf = c.resources.DISPBUF;
        let mut comm = c.resources.COMM;

        match event {
            SpiEvent::Disp => {
                if let Some(disp_buf) = disp_buf.lock(|b| b.take()) {
                    disp.draw(&disp_buf).ok();
                }
            },
            SpiEvent::Rfid => {
                if let Ok(atqa) = rfid.reqa() {
                    if let Ok(info) = rfid.select(&atqa) {
                        let mut uid = Vec::<u8, U10>::new();
                        uid.extend_from_slice(info.uid().bytes()).unwrap();
                        comm.lock(|c| c.send(Event::Rfid(uid)).ok());
                    }
                }
            }
        }
    }

    #[task(
        resources = [COMM, COMMTIMEOUT, DISPBUF, BTN, BEEPER],
        spawn = [spi],
        schedule = [tick],
        priority = 2)]
    fn tick(c: tick::Context) {
        static mut RFID_TICK: usize = 0;

        let mut comm = c.resources.COMM;
        let mut comm_timeout = c.resources.COMMTIMEOUT;
        let mut disp_buf = c.resources.DISPBUF;
        let mut beeper = c.resources.BEEPER;
        let btn = c.resources.BTN;

        beeper.lock(|b| b.tick());

        if let Some(state) = btn.poll() {
            comm.lock(|c| c.send(Event::Button(state)).ok());
        }

        if *RFID_TICK == 0 {
            *RFID_TICK = 100 / 5;

            c.spawn.spi(SpiEvent::Rfid).ok();
        } else {
            *RFID_TICK -= 1;
        }

        let is_timeout = comm_timeout.lock(|comm_timeout| {
            *comm_timeout += 1;

            *comm_timeout == 400
        });

        if is_timeout {
            disp_buf.lock(|disp_buf| {
                set_disp_buf(disp_buf, COMM_ERROR_IMG);
            });

            c.spawn.spi(SpiEvent::Disp).ok();
        }

        c.schedule.tick(c.scheduled + (5 * MS).cycles()).unwrap();
    }

    #[interrupt(resources = [COMM, COMMTIMEOUT, DISPBUF, LED, BEEPER], spawn = [spi], priority = 3)]
    fn USART3(c: USART3::Context) {
        let mut comm = c.resources.COMM;
        let mut comm_timeout = c.resources.COMMTIMEOUT;
        let mut disp_buf = c.resources.DISPBUF;
        let led = c.resources.LED;
        let mut beeper = c.resources.BEEPER;

        if let Some(cmd) = comm.handle_rx() {
            match cmd {
                Command::Reset => {
                    comm.clear_events();
                    led.set_low();
                    beeper.stop();
                },
                Command::Led(true) => led.set_high(),
                Command::Led(false) => led.set_low(),
                Command::Beep(notes) => {
                    beeper.stop();

                    for n in notes {
                        beeper.play(n)
                    }
                }
                Command::Display(data) => {
                    set_disp_buf(&mut *disp_buf, data);
                    c.spawn.spi(SpiEvent::Disp).ok();
                }
                _ => {}
            };

            comm.respond();

            *comm_timeout = 0;
        }
    }

    extern "C" {
        fn USART1();
        fn USART2();
    }
};

fn set_disp_buf(disp_buf: &mut Option<[u8; 1024]>, data: &[u8]) {
    let mut arr = core::mem::MaybeUninit::<[u8; 1024]>::uninit();
    unsafe { core::ptr::copy(data.as_ptr(), arr.as_mut_ptr() as *mut u8, data.len()); }
    *disp_buf = Some(unsafe { arr.assume_init() });
}

enum SpiEvent {
    Disp,
    Rfid,
}
