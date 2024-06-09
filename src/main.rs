#![no_std]
#![no_main]

// pick a panicking behavior
use panic_halt as _; // you can put a breakpoint on `rust_begin_unwind` to catch panics
                     // use panic_abort as _; // requires nightly
                     // use panic_itm as _; // logs messages over ITM; requires ITM support
                     // use panic_semihosting as _; // logs messages to the host stderr; requires a debugger
use defmt_rtt as _;

use core::cell::RefCell;
use core::fmt::Write;
use core::ops::DerefMut;

use cortex_m::interrupt::{free, Mutex};
use cortex_m_rt::entry;

mod app;
mod fsr;
mod imu_fsr_stm32g4;
mod indicator;
mod potensio;

static G_APP: Mutex<
    RefCell<
        Option<
            app::App<
                imu_fsr_stm32g4::Led0,
                imu_fsr_stm32g4::Led1,
                imu_fsr_stm32g4::Led2,
                imu_fsr_stm32g4::Uart1,
                imu_fsr_stm32g4::LocalClock,
            >,
        >,
    >,
> = Mutex::new(RefCell::new(None));

//　タイマ割り込みでIMU等読み取り[App]
// FSRのADC結果を即時反映するためにはDMA完了割り込みがよさそう？

// 受信割り込みでuart処理[これもApp内でおこなう]

// static adc_data:[u16; 4] = [7; 4];

#[entry]
fn main() -> ! {
    use stm32g4::stm32g431;

    defmt::info!("Hello from STM32G4!");
    // stm32f401モジュールより、ペリフェラルの入り口となるオブジェクトを取得する。
    let perip = stm32g431::Peripherals::take().unwrap();
    let mut core_perip = stm32g431::CorePeripherals::take().unwrap();

    imu_fsr_stm32g4::clock_init(&perip);
    imu_fsr_stm32g4::adc2_init(&perip);

    let adc_data: [u16; 4] = [7; 4];
    let dma_buf_addr: u32 = adc_data.as_ptr() as u32;
    imu_fsr_stm32g4::dma_init(&perip, &mut core_perip, dma_buf_addr);
    imu_fsr_stm32g4::dma_adc2_start(&perip);

    // init g peripheral
    imu_fsr_stm32g4::init_g_peripheral(perip);

    let led0 = imu_fsr_stm32g4::Led0::new();
    led0.init();
    let led1 = imu_fsr_stm32g4::Led1::new();
    led1.init();
    let led2 = imu_fsr_stm32g4::Led2::new();
    led2.init();
    let uart = imu_fsr_stm32g4::Uart3::new();
    uart.init();
    let spi = imu_fsr_stm32g4::SPI2::new();
    spi.init();

    let mut uart_rs854 = imu_fsr_stm32g4::Uart1::new();
    uart_rs854.init();
    let clock: imu_fsr_stm32g4::LocalClock = imu_fsr_stm32g4::LocalClock::new();
    clock.init();

    let app = app::App::new(led0, led1, led2, uart, spi, uart_rs854, clock);
    free(|cs| G_APP.borrow(cs).replace(Some(app)));

    let mut t = 0;
    free(
        |cs| match imu_fsr_stm32g4::G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                t = perip.TIM3.cnt.read().cnt().bits();
            }
        },
    );
    let mut prev = t;
    let mut cnt = 0;
    let mut adc_data_fir: [u16; 4] = [0; 4];
    loop {
        free(
            |cs| match imu_fsr_stm32g4::G_PERIPHERAL.borrow(cs).borrow().as_ref() {
                None => (),
                Some(perip) => {
                    t = perip.TIM3.cnt.read().cnt().bits();
                }
            },
        );

        if t.wrapping_sub(prev) > 50 {
            for i in 0..4 {
                adc_data_fir[i] = (adc_data_fir[i] as f32 * 0.9 + adc_data[i] as f32 * 0.1) as u16;
            }
            cnt += 1;
            if cnt > 100 {
                free(|cs| match G_APP.borrow(cs).borrow_mut().deref_mut() {
                    None => (),
                    Some(app) => {
                        app.periodic_task();
                        app.read_imu_task();
                    }
                });

                defmt::error!("error from defmt");
                defmt::warn!("warn from defmt");
                defmt::info!("info from defmt");

                defmt::info!(
                    "{{\"FSR\":[{}, {}, {}, {}]}}",
                    adc_data_fir[3],
                    adc_data_fir[0],
                    adc_data_fir[1],
                    adc_data_fir[2]
                );

                // uart.write_str("hello ");
                // write!(uart, "{} + {} = {}\r\n", 2, 4, 2+4);
                // unsafe {
                //     write!(
                //         uart,
                //         "{{\"FSR\":[{}, {}, {}, {}]}}\r\n",
                //         adc_data_fir[3], adc_data_fir[0], adc_data_fir[1], adc_data_fir[2]
                //     );
                // }
                cnt = 0;
            }
            prev = t;
        }
    }
}
