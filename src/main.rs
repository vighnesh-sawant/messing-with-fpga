#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::spi::{Config as SpiConfig, Spi};
use embassy_rp::uart::{Config as UartConfig, Uart, InterruptHandler as UartInterruptHandler};
use embassy_rp::usb::{Driver, InterruptHandler as UsbInterruptHandler};
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::{PIN_0, PIN_1, UART0, USB};
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::{Builder, Config};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}

// --- Memory Buffers ---
static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESC:    StaticCell<[u8; 256]> = StaticCell::new();
static MSOS_DESC:   StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 64]>  = StaticCell::new();
static STATE:       StaticCell<State>     = StaticCell::new();

// --- Interrupts ---
bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => UsbInterruptHandler<USB>;
    UART0_IRQ => UartInterruptHandler<UART0>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // --- Setup USB ---
    let driver = Driver::new(p.USB, Irqs);
    let mut config = Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Vighnesh");
    config.product = Some("FPGA");

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

    
    // ---  FPGA Control Pins ---
    let mut fpga_pwr = Output::new(p.PIN_12, Level::Low); 
    let mut fpga_en  = Output::new(p.PIN_13, Level::Low); 
    let mut fpga_cs  = Output::new(p.PIN_1, Level::High);   

    let mut usb_buf = [0u8; 64]; 
    let mut led = Output::new(p.PIN_4, Level::Low); 
    {
    // --- Setup SPI  ---
    let mut spi_config = SpiConfig::default();
    spi_config.frequency = 10_000_000; 

    let mut spi = Spi::new(
        p.SPI0, 
        p.PIN_2, // CLK
        p.PIN_3, // MOSI
        p.PIN_0, // MISO
        p.DMA_CH1, 
        p.DMA_CH2,  
        spi_config
    );

 


    
    // Initial Power Sequence
    Timer::after(Duration::from_millis(10)).await;
    fpga_pwr.set_high();
    fpga_en.set_high(); 
    fpga_cs.set_low();  
    Timer::after(Duration::from_millis(10)).await;
    fpga_cs.set_high(); 

    let mut total_bytes_sent: usize = 0;
    const BITSTREAM_SIZE: usize = 46408;
 


    while total_bytes_sent < BITSTREAM_SIZE {
        match cdc_acm_class.read_packet(&mut usb_buf).await {
            Ok(n) => {
                // Send raw data directly to SPI
                fpga_cs.set_low();
                spi.write(&usb_buf[0..n]).await.unwrap(); 
                fpga_cs.set_high();
                total_bytes_sent += n;
            }
            Err(_) => {
                cdc_acm_class.wait_connection().await;
            }
        }
    }
    
}
   // ---  Setup UART ---
    let mut uart_config = UartConfig::default();
    uart_config.baudrate = 115200; 
    Output::new(unsafe{ PIN_0::steal()} , Level::High);  

    let mut fpga_led_rst = Output::new( p.PIN_14 , Level::Low);  
    
    let mut uart = Uart::new(p.UART0, unsafe {PIN_0::steal()}, unsafe {PIN_1::steal()}, Irqs, p.DMA_CH3, p.DMA_CH4, uart_config);
    let mut cmd_buf = [0u8; 128]; 
    let mut cmd_len = 0;

    loop {

        match cdc_acm_class.read_packet(&mut usb_buf).await {
            Ok(n) if n > 0 => {
                for i in 0..n {
                    if cmd_len < cmd_buf.len() {
                        let byte = usb_buf[i];
                        cmd_buf[cmd_len] = byte;
                        cmd_len += 1;

                        if byte == b'\n' || byte == b'\r'  {
                            
                            let full_command = &cmd_buf[0..cmd_len];
                            
                            if !full_command.is_empty() {
                                match full_command[0] {
                                    b'$' => {
                                        fpga_led_rst.set_low();
                                        // Send everything between '$' and '\n' to UART
                                        if cmd_len > 2 {
                                            let data = &full_command[1..cmd_len-1]; 
                                            uart.write(data).await.unwrap();
                                            let _ = cdc_acm_class.write_packet(data).await;
                                            let _ = cdc_acm_class.write_packet(b"SENT TO FPGA \r\n").await;
                                        }
                                    },
                                    b'r' => {
                                        fpga_led_rst.set_high();
                                        let _ = cdc_acm_class.write_packet(b"SENT FPGA RESET SIGNAL\r\n").await;
                                    },
                                    b'1' => {
                                        led.set_high();
                                        let _ = cdc_acm_class.write_packet(b"LED ON\r\n").await;
                                    },
                                    b'0' => {
                                        led.set_low();
                                        let _ = cdc_acm_class.write_packet(b"LED OFF\r\n").await;
                                    },
                                    _ => {}
                                }
                            }

                            cmd_len = 0;
                        }
                    } else {
                        // Buffer overflow protection: Reset if we get too much garbage
                        cmd_len = 0; 
                    }
                }
            }
            Ok(_) => {},
            Err(_) => cdc_acm_class.wait_connection().await,
        }
    }
}

#[embassy_executor::task]
async fn usb_task(mut usb: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) {
    usb.run().await;
}
