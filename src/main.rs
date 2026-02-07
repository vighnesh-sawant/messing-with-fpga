#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::spi::{Config as SpiConfig, Spi};
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_rp::bind_interrupts;
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::{Builder, Config};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}


static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESC:    StaticCell<[u8; 256]> = StaticCell::new();
static MSOS_DESC:   StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 64]>  = StaticCell::new();
static STATE:       StaticCell<State>     = StaticCell::new();

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<embassy_rp::peripherals::USB>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // --- 1. Setup USB ---
    let driver = Driver::new(p.USB, Irqs);
    let mut config = Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Vighnesh");
    config.product = Some("FPGA Streamer"); 

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESC.init([0; 256]),
        BOS_DESC.init([0; 256]),
        MSOS_DESC.init([0; 256]),
        CONTROL_BUF.init([0; 64]),
    );
    let mut cdc_acm_class = CdcAcmClass::new(&mut builder, STATE.init(State::new()), 64);
    let usb = builder.build();
    spawner.spawn(usb_task(usb)).unwrap();

    let led = Output::new(p.PIN_4, Level::High); 
    spawner.spawn(blinker(led)).unwrap();

    // --- 2. Setup SPI ---
    let mut spi_config = SpiConfig::default();
    spi_config.frequency = 10_000_000; 

    let mut spi = Spi::new(
        p.SPI0, 
        p.PIN_2, // CLK
        p.PIN_3, // MOSI
        p.PIN_0, // MISO
        p.DMA_CH1, // TX DMA
        p.DMA_CH2, // RX DMA 
        spi_config
    );

    //fpga init
    let mut fpga_pwr = Output::new(p.PIN_12, Level::Low); // Power 
    let mut fpga_en  = Output::new(p.PIN_13, Level::Low);  // Hold Reset 
    let mut fpga_cs = Output::new(p.PIN_1, Level::High);   
    Timer::after(Duration::from_millis(3)).await;
    fpga_pwr.set_high();
    fpga_en.set_high();
    fpga_cs.set_low();
    Timer::after(Duration::from_millis(3)).await;
    fpga_cs.set_low();
    Timer::after(Duration::from_millis(3)).await;




loop {
        let mut buf = [0u8; 64]; 

        match cdc_acm_class.read_packet(&mut buf).await {
            Ok(n) => {
                
                fpga_cs.set_low();
                
                spi.write(&buf[0..n]).await.unwrap(); 
                
                fpga_cs.set_high();
            }
            Err(_) => {
                cdc_acm_class.wait_connection().await;
            }
        }
    }

}

#[embassy_executor::task]
async fn usb_task(mut usb: embassy_usb::UsbDevice<'static, Driver<'static, embassy_rp::peripherals::USB>>) {
    usb.run().await;
}

#[embassy_executor::task]
async fn blinker(mut led: Output<'static>) {
    loop {
        led.set_high();
        Timer::after(Duration::from_millis(1000)).await;
        led.set_low();
        Timer::after(Duration::from_millis(1000)).await;
    }
}
