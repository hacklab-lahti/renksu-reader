use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use embedded_hal::blocking::spi;

/// Simple wrapper to share a SPI bus between multiple drivers. Will panic if two drivers attempt to
/// access it at the same time.
pub struct SharedSpi<SPI> {
    spi: UnsafeCell<SPI>,
    busy: AtomicBool,
}

impl<SPI> SharedSpi<SPI> {
    pub fn new(spi: SPI) -> Self {
        SharedSpi {
            spi: UnsafeCell::new(spi),
            busy: AtomicBool::from(false),
        }
    }

    fn lock<R, F: FnOnce(&mut SPI) -> R>(&self, f: F) -> R {
        self.busy.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .expect("SPI bus conflict");

        let r = f(unsafe { &mut *self.spi.get() });

        self.busy.store(false, Ordering::SeqCst);

        r
    }
}

unsafe impl<SPI> Sync for SharedSpi<SPI> { }

impl<SPI> spi::Transfer<u8> for &SharedSpi<SPI>
where
    SPI: spi::Transfer<u8>
{
    type Error = SPI::Error;

    fn transfer<'w>(&mut self, words: &'w mut [u8]) -> Result<&'w [u8], Self::Error> {
        self.lock(move |spi| spi.transfer(words))
    }
}

impl<SPI> spi::Write<u8> for &SharedSpi<SPI>
where
    SPI: spi::Write<u8>
{
    type Error = SPI::Error;

    fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
        self.lock(|spi| spi.write(words))
    }
}
