#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_mspm0::Config;
use embassy_mspm0::flash::Flash;
use embassy_mspm0::gpio::{Level, Output};
use embassy_time::Timer;
use {defmt_rtt as _, panic_halt as _};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    info!("Hello world!");
    let p = embassy_mspm0::init(Config::default());

    let mut led1 = Output::new(p.PB10, Level::Low);
    let mut flash = Flash::new(p.FLASHCTL);
    led1.set_inversion(true);
    let mut i = 0u8;

    loop {
        Timer::after_millis(400).await;

        i = i.wrapping_add(1);
        info!("Write to flash: {}", i);

        unwrap!(flash.erase(0x10000, 0x16000));
        unwrap!(flash.write(0x10000, &[i; 0x2000]));
        let mut buf = [0; 0x2000];
        unwrap!(flash.read(0x10000, &mut buf));
        defmt::assert_eq!(buf, [i; 0x2000]);

        info!("Toggle");
        led1.toggle();
    }
}
