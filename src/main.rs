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

use stm32g4::stm32g431::{interrupt, uart4};
use stm32g4::stm32g431::Interrupt::USART1;
use stm32g4::stm32g431::Interrupt::DMA1_CH1;
use stm32g4::stm32g431::Interrupt::TIM3;

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
#[interrupt]
fn DMA1_CH1() {

    free(|cs| match imu_fsr_stm32g4::G_PERIPHERAL.borrow(cs).borrow().as_ref() {
        None => {
            defmt::info!("Peripheral is not initialized yet. Return");
            return;
        },
        Some(perip) => {
            let dma = &perip.DMA1;
            // TCIFで割り込みが入っていることの確認
            if dma.isr.read().htif1().bit_is_set(){
                // Ok
            } else {
                // 何かおかしいのでreturn
                defmt::info!("something went wrong. TCIF bit is not set");
                return;
            }
        }
    });

    free(|cs| match G_APP.borrow(cs).borrow_mut().deref_mut() {
        None => {
            // defmt::info!("App is not initialized yet. Return");
            return;
        },
        Some(app) => {
            // defmt::info!("dma interrupt");
            app.update_fsr_task();
            app.read_imu_task();
        }
    });

}

// 受信割り込みでbufferに入れる
#[interrupt]
fn USART1() {

    let mut recieved_flag = false;
    // FIFOの最大サイズ+1までループでチェックする
    for _ in 0..=9 {
        let mut data:u8 = 0;
        let mut return_flag = false;
        free(|cs| match imu_fsr_stm32g4::G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => {
                // 初期化がまだなのでreturnしてよい。
                defmt::info!("Not initialized yet. Return");
                return_flag = true;
            },
            Some(perip) => {
                let uart = &perip.USART1;
                // RXFNEで割り込みが入っていることの確認
                if uart.isr.read().rxne().bit_is_clear() { // RXFNE
                    // bufferが空になったらreturn
                    return_flag = true;
                } else {
                    data = uart.rdr.read().rdr().bits() as u8;
                    defmt::info!("get: 0x{:x}", data);
                    recieved_flag = true;
                }
            }
        });
        // free内部からいきなりreturnできないのでflagを使う
        if return_flag {
            if recieved_flag {
                defmt::info!("recieved flag is true");
                break;
            }else{
                return;
            }
        }
        free(|cs| match G_APP.borrow(cs).borrow_mut().deref_mut() {
            None => {
                // 初期化がまだなのでreturnしてよい。
                defmt::info!("Not initialized yet. Return");
                return_flag = true;
            },
            Some(app) => {
                app.enque_uart(data);
                defmt::info!("enqueue");
            }
        });
        if return_flag {
            return;
        }    
    }

    free(|cs| match G_APP.borrow(cs).borrow_mut().deref_mut() {
        None => (),
        Some(app) => {
            // 👺はホントは受信完了時にするのがよさそう？
            app.parse_uart_task();
            defmt::info!("parse uart task finished.");
        }
    });

    // // ここまで来ているということは正常に空にできていない
    // defmt::error!("Something went wrong when clearing fifo.");
    // unreachable!();
    // free(|cs| match imu_fsr_stm32g4::G_PERIPHERAL.borrow(cs).borrow().as_ref() {
    //     None => (),
    //     Some(perip) => {
    //         //    The RXFNE flag can also be cleared by writing 1 to the RXFRQ in the USART_RQR register
    //         let uart = &perip.USART1;
    //         uart.rqr.write(|w| w.rxfrq().set_bit());
    //     }
    // });

}

#[interrupt]
fn TIM3() {
    imu_fsr_stm32g4::clear_tim3_uif();

    free(|cs| match G_APP.borrow(cs).borrow_mut().deref_mut() {
        None => {
            defmt::info!("App is not initialized yet. Return");
            return;
        },
        Some(app) => {
            defmt::warn!("toggle");
            //割り込み内でしかcsが取れない？👺
            // app.init();
            app.periodic_task();
        }
    });


    // free(|cs| match G_APP.borrow(cs).borrow_mut().deref_mut() {
    //     None => (),
    //     Some(app) => {
    //         // 👺はホントは受信完了時にするのがよさそう？
    //         app.parse_uart_task();
    //         defmt::info!("parse uart task finished.");
    //     }
    // });


}


// dxl.parse_data(はブロッキングなのでmainで呼ぶはず


    
#[entry]
fn main() -> ! {
    use stm32g4::stm32g431;

    defmt::info!("Hello from STM32G4!");
    // stm32f401モジュールより、ペリフェラルの入り口となるオブジェクトを取得する。
    let perip = stm32g431::Peripherals::take().unwrap();
    let mut core_perip = stm32g431::CorePeripherals::take().unwrap();

    imu_fsr_stm32g4::clock_init(&perip, &mut core_perip);
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

    free(|cs|{
        let app = app::App::new(led0, led1, led2, uart, spi, uart_rs854, clock);
        app.init();
        G_APP.borrow(cs).replace(Some(app));
    });
    // free(|cs| G_APP.borrow(cs).replace(Some(app)));

    
    loop {
    }


    // let mut t = 0;
    // free(
    //     |cs| match imu_fsr_stm32g4::G_PERIPHERAL.borrow(cs).borrow().as_ref() {
    //         None => (),
    //         Some(perip) => {
    //             t = perip.TIM3.cnt.read().cnt().bits();
    //         }
    //     },
    // );
    // let mut prev = t;
    // let mut cnt = 0;
    // let mut adc_data_fir: [u16; 4] = [0; 4];
    // loop {
    //     free(
    //         |cs| match imu_fsr_stm32g4::G_PERIPHERAL.borrow(cs).borrow().as_ref() {
    //             None => (),
    //             Some(perip) => {
    //                 t = perip.TIM3.cnt.read().cnt().bits();
    //             }
    //         },
    //     );

    //     if t.wrapping_sub(prev) > 50 {
    //         for i in 0..4 {
    //             adc_data_fir[i] = (adc_data_fir[i] as f32 * 0.9 + adc_data[i] as f32 * 0.1) as u16;
    //         }
    //         cnt += 1;
    //         if cnt > 100 {
    //             free(|cs| match G_APP.borrow(cs).borrow_mut().deref_mut() {
    //                 None => (),
    //                 Some(app) => {
    //                     app.periodic_task();
    //                     app.read_imu_task();
    //                 }
    //             });

    //             defmt::error!("error from defmt");
    //             defmt::warn!("warn from defmt");
    //             defmt::info!("info from defmt");

    //             defmt::info!(
    //                 "{{\"FSR\":[{}, {}, {}, {}]}}",
    //                 adc_data_fir[3],
    //                 adc_data_fir[0],
    //                 adc_data_fir[1],
    //                 adc_data_fir[2]
    //             );

    //             // uart.write_str("hello ");
    //             // write!(uart, "{} + {} = {}\r\n", 2, 4, 2+4);
    //             // unsafe {
    //             //     write!(
    //             //         uart,
    //             //         "{{\"FSR\":[{}, {}, {}, {}]}}\r\n",
    //             //         adc_data_fir[3], adc_data_fir[0], adc_data_fir[1], adc_data_fir[2]
    //             //     );
    //             // }
    //             cnt = 0;
    //         }
    //         prev = t;
    //     }
    // }
}
