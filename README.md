# renksu-reader

RFID tag reader / display / doorbell module for use with [renksu][renksu].

## Hardware

The hardware is the following, all of which are commonly available as pre-made modules if need be:

- **STM32F103C8T6** microcontroller
- **MFRC522** RFID reader in SPI mode
- **SH1106** 128x64 OLED display in SPI mode
- **MAX485** RS-485 transceiver in half-duplex mode
- Generic button, LED, piezo speaker
- Board to tie everything together

## Pin connections

Below are the required connections from the microcontroller to the various other devices. The three
SPI bus pins are shared between the RFID reader and OLED. The MAX485 uses a 5V supply and is
therefore connected to 5V tolerant pins. These pins are all conveniently located on one side of an
STM32 "Blue pill" board.

    GND   GND
    GND   GND
    3V3   VCC
    R     -
    PB11  MAX485 RO (RX)
    PB10  MAX485 DI (TX)
    PB1   MAX485 DE
    PB0   LED
    PA7   MOSI
    PA6   MISO
    PA5   SCK
    PA4   BUTTON
    PA3   RFID CS
    PA2   LCD DC
    PA1   LCD CS
    PA0   SPEAKER
    PC15  -
    PC14  -
    PC13  (built-in LED, not really used)
    VBAT  -

[renksu]: https://github.com/hacklab-lahti/renksu
