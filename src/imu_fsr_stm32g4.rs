// interfaces
use crate::fsr::Fsr;
use crate::indicator::Indicator;
use crate::potensio::Potensio;
use dynamixel_f_rs::BufferInterface;
use dynamixel_f_rs::Clock;

//
use core::cell::RefCell;
use core::fmt::{self, Write};
use core::time::Duration;

use stm32g4::stm32g431::CorePeripherals;
use stm32g4::stm32g431::Interrupt;
use stm32g4::stm32g431::Peripherals;
use stm32g4::stm32g431::NVIC;

use cortex_m::interrupt::{free, Mutex};

pub static G_PERIPHERAL: Mutex<RefCell<Option<stm32g4::stm32g431::Peripherals>>> =
    Mutex::new(RefCell::new(None));

pub fn init_g_peripheral(perip: Peripherals) {
    free(|cs| G_PERIPHERAL.borrow(cs).replace(Some(perip)));
}

pub fn clock_init(perip: &Peripherals, core_perip: &mut CorePeripherals) {
    perip.RCC.cr.modify(|_, w| w.hsebyp().bypassed());
    perip.RCC.cr.modify(|_, w| w.hseon().on());
    while perip.RCC.cr.read().hserdy().is_not_ready() {}

    // Disable the PLL
    perip.RCC.cr.modify(|_, w| w.pllon().off());
    // Wait until PLL is fully stopped
    while perip.RCC.cr.read().pllrdy().is_ready() {}
    perip.RCC.pllcfgr.modify(|_, w| w.pllsrc().hse());
    perip.RCC.pllcfgr.modify(|_, w| w.pllm().div12());
    // perip.RCC.pllcfgr.modify(|_, w| w.plln().div85());
    perip.RCC.pllcfgr.modify(|_, w| w.plln().div70());
    perip.RCC.pllcfgr.modify(|_, w| w.pllr().div2());

    perip.RCC.cr.modify(|_, w| w.pllon().on());
    while perip.RCC.cr.read().pllrdy().is_not_ready() {}
    perip.RCC.pllcfgr.modify(|_, w| w.pllren().set_bit());

    perip
        .FLASH
        .acr
        .modify(|_, w| unsafe { w.latency().bits(4) });
    while perip.FLASH.acr.read().latency().bits() != 4 {
        defmt::info!("latency bit: {}", perip.FLASH.acr.read().latency().bits());
    }

    perip.RCC.cfgr.modify(|_, w| w.sw().pll());
    // perip.RCC.cfgr.modify(|_, w| w.sw().hse());
    defmt::debug!("sw bit: {}", perip.RCC.cfgr.read().sw().bits());
    while !perip.RCC.cfgr.read().sw().is_pll() {}
    while !perip.RCC.cfgr.read().sws().is_pll() {
        defmt::info!("sw bit: {}", perip.RCC.cfgr.read().sw().bits());
        defmt::info!("sws bit: {}", perip.RCC.cfgr.read().sws().bits());
    }
    // while !perip.RCC.cfgr.read().sws().is_hse() {}

    perip.RCC.apb1enr1.modify(|_, w| w.tim3en().enabled());
    perip.RCC.apb1enr1.modify(|_, w| w.tim6en().enabled());

    let tim3 = &perip.TIM3;
    // tim3.psc.modify(|_, w| unsafe { w.bits(170 - 1) });
    tim3.psc.modify(|_, w| unsafe { w.bits(14_000 - 1) });
    tim3.arr.modify(|_, w| unsafe { w.bits(10_000 - 1) });    // 1Hz
    tim3.dier.modify(|_, w| w.uie().set_bit());
    tim3.cr1.modify(|_, w| w.cen().set_bit());

    let tim6 = &perip.TIM6;
    tim6.psc.modify(|_, w| unsafe { w.bits(140 - 1) });
    tim6.arr.modify(|_, w| unsafe { w.bits(1_000 - 1) }); // 1kHz
    tim6.dier.modify(|_, w| w.uie().set_bit());
    tim6.cr2.modify(|_, w| unsafe { w.mms().bits(0b010) });

    // 割り込み設定
    unsafe {
        core_perip.NVIC.set_priority(Interrupt::USART1, 0);
        NVIC::unmask(Interrupt::USART1);
        core_perip.NVIC.set_priority(Interrupt::TIM3, 2);
        NVIC::unmask(Interrupt::TIM3);
    }

}

pub fn clear_tim3_uif() {
    free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
        None => (),
        Some(perip) => {
            let tim3 = &perip.TIM3;
            tim3.sr.modify(|_, w| w.uif().clear_bit());
        }
    });
}


pub fn dma_init(perip: &Peripherals, core_perip: &mut CorePeripherals, address: u32) {
    // DMAの電源投入(クロックの有効化)
    // perip.RCC.ahb1rstr.modify(|_, w| w.dmamux1rst().reset());
    // perip.RCC.ahb1rstr.modify(|_, w| w.dma1rst().reset());
    perip.RCC.ahb1rstr.modify(|_, w| w.dmamux1rst().set_bit());
    perip.RCC.ahb1rstr.modify(|_, w| w.dma1rst().set_bit());
    perip.RCC.ahb1rstr.modify(|_, w| w.dmamux1rst().clear_bit());
    perip.RCC.ahb1rstr.modify(|_, w| w.dma1rst().clear_bit());
    perip.RCC.ahb1enr.modify(|_, w| w.dmamuxen().set_bit());
    perip.RCC.ahb1enr.modify(|_, w| w.dma1en().set_bit());

    perip.DMA1.ccr1.modify(|_, w| unsafe { w.pl().bits(0b10) }); // priority level 2
    perip
        .DMA1
        .ccr1
        .modify(|_, w| unsafe { w.msize().bits(0b01) }); // 16bit
    perip
        .DMA1
        .ccr1
        .modify(|_, w| unsafe { w.psize().bits(0b01) }); // 16bit
    perip.DMA1.ccr1.modify(|_, w| w.circ().set_bit()); // circular mode
    perip.DMA1.ccr1.modify(|_, w| w.minc().set_bit()); // increment memory ptr
    perip.DMA1.ccr1.modify(|_, w| w.pinc().clear_bit()); // not increment periph  ptr
    perip.DMA1.ccr1.modify(|_, w| w.mem2mem().clear_bit()); // memory-to-memory mode
    perip.DMA1.ccr1.modify(|_, w| w.dir().clear_bit()); // read from peripheral
    perip.DMA1.ccr1.modify(|_, w| w.teie().clear_bit()); // transfer error interrupt enable
    perip.DMA1.ccr1.modify(|_, w| w.htie().clear_bit()); // half transfer interrupt enable
    perip.DMA1.ccr1.modify(|_, w| w.tcie().set_bit()); // transfer complete interrupt enable

    // For category 2 devices:
    // • DMAMUX channels 0 to 5 are connected to DMA1 channels 1 to 6
    // • DMAMUX channels 6 to 11 are connected to DMA1 channels 1 to 6
    // DMA1 ch1 -> DMAMUX ch6
    perip
        .DMAMUX
        .c0cr
        .modify(|_, w| unsafe { w.dmareq_id().bits(36) }); // Table.91 36:ADC2
    perip.DMAMUX.c0cr.modify(|_, w| w.ege().set_bit()); // Enable generate event

    let adc = &perip.ADC2;
    let adc_data_register_addr = &adc.dr as *const _ as u32;
    // let adc_dma_buf_addr : u32 = adc_dma_buf as *const [u16; 4] as u32;
    // perip.DMA1.cpar1.modify(|_, w| unsafe { w.pa().bits(*adc.dr.as_ptr()) });   // peripheral address
    perip
        .DMA1
        .cpar1
        .modify(|_, w| unsafe { w.pa().bits(adc_data_register_addr) }); // peripheral address
                                                                        // perip.DMA1.cndtr1.modify(|_, w| unsafe { w.ndt().bits(adc_dma_buf.len() as u16) }); // num
    perip.DMA1.cndtr1.modify(|_, w| unsafe { w.ndt().bits(4) }); // num
                                                                 // perip.DMA1.cmar1.modify(|_, w| unsafe { w.ma().bits(adc_dma_buf_addr) });      // memory address
    perip
        .DMA1
        .cmar1
        .modify(|_, w| unsafe { w.ma().bits(address) }); // memory address

    
    // 割り込み設定
    unsafe{
        core_perip.NVIC.set_priority(Interrupt::DMA1_CH1, 1);
        NVIC::unmask(Interrupt::DMA1_CH1);
    }

}

pub fn adc2_init(perip: &Peripherals) {
    // GPIOポートの電源投入(クロックの有効化)
    perip.RCC.ahb2enr.modify(|_, w| w.gpioaen().set_bit());
    perip.RCC.ahb2enr.modify(|_, w| w.gpioben().set_bit());

    perip.RCC.ahb2enr.modify(|_, w| w.adc12en().set_bit());
    perip.RCC.ccipr.modify(|_, w| w.adc12sel().system()); // clock source setting

    // gpioモード変更
    perip.GPIOA.moder.modify(|_, w| w.moder5().analog());
    perip.GPIOA.moder.modify(|_, w| w.moder6().analog());
    perip.GPIOA.moder.modify(|_, w| w.moder7().analog());
    perip.GPIOB.moder.modify(|_, w| w.moder2().analog());

    let adc = &perip.ADC2;
    adc.cfgr.modify(|_, w| w.res().bits12()); // Resolution setting
    adc.cfgr.modify(|_, w| w.align().right()); // Data align setting
    adc.cfgr.modify(|_, w| w.ovrmod().overwrite()); // Overrun mode

    adc.cfgr.modify(|_, w| w.cont().single()); // single or continuous
                                               // adc.cfgr.modify(|_, w| w.cont().continuous());   // single or continuous
    adc.cfgr.modify(|_, w| w.discen().disabled()); // single or continuous
                                                   // adc.cfgr.modify(|_, w| w.discen().enabled());   // single or continuous
                                                   // DISCEN = 1 and CONT = 1 is not allowed.
                                                   // adc.cfgr.modify(|_, w| w.discnum().bits(4-1));   // 0 means 1 length

    adc.cfgr.modify(|_, w| w.dmacfg().circular()); // dma oneshot or circular
    adc.cfgr.modify(|_, w| w.dmaen().enabled()); // dma enable
                                                 // 1周は実行したいが，常に変換しつづけるのは困る
    adc.cfgr.modify(|_, w| w.extsel().tim6_trgo()); // dma enable
    adc.cfgr.modify(|_, w| w.exten().rising_edge()); // dma enable

    perip
        .ADC12_COMMON
        .ccr
        .modify(|_, w| unsafe { w.presc().bits(0b0010) }); // Clock prescaler setting

    adc.cr.modify(|_, w| w.deeppwd().disabled()); // Deep power down setting
    adc.cr.modify(|_, w| w.advregen().enabled()); // Voltage regulator setting
                                                  // adc.ier.modify(|_, w| w.eocie().enabled());   // End of regular conversion interrupt setting
    adc.ier.modify(|_, w| w.eocie().disabled()); // End of regular conversion interrupt setting
    adc.ier.modify(|_, w| w.ovrie().enabled()); // Overrun interrupt setting
                                                // // ADC voltage regulator start-up time 20us
    let mut t = perip.TIM3.cnt.read().cnt().bits();
    let prev = t;
    while t.wrapping_sub(prev) >= 10 {
        t = perip.TIM3.cnt.read().cnt().bits();
    }
    // P.604 21.4.8 calibration
    assert!(adc.cr.read().aden().is_enable() == false);
    adc.cr.modify(|_, w| w.adcal().calibration()); // Start calibration
    while !adc.cr.read().adcal().is_complete() {} // Wait for calibration complete

    adc.smpr1.modify(|_, w| w.smp3().cycles24_5()); // sampling time selection
    adc.smpr1.modify(|_, w| w.smp4().cycles24_5()); // sampling time selection
    adc.smpr2.modify(|_, w| w.smp12().cycles24_5()); // sampling time selection
    adc.smpr2.modify(|_, w| w.smp13().cycles24_5()); // sampling time selection

    adc.sqr1.modify(|_, w| w.l().bits(4 - 1)); // Regular channel sequence length. 0 means 1 length
    adc.sqr1.modify(|_, w| unsafe { w.sq1().bits(3) }); // 1st conversion in regular sequence
    adc.sqr1.modify(|_, w| unsafe { w.sq2().bits(4) }); // 1st conversion in regular sequence
    adc.sqr1.modify(|_, w| unsafe { w.sq3().bits(12) }); // 1st conversion in regular sequence
    adc.sqr1.modify(|_, w| unsafe { w.sq4().bits(13) }); // 1st conversion in regular sequence
}

pub fn dma_adc2_start(perip: &Peripherals) {
    // enable DMA
    perip.DMA1.ccr1.modify(|_, w| w.en().set_bit());

    let adc = &perip.ADC2;
    // enable ADC
    adc.isr.modify(|_, w| w.adrdy().set_bit());
    adc.cr.modify(|_, w| w.aden().enable()); // ADC enable control
    while adc.isr.read().adrdy().is_not_ready() {
        // Wait for ADC ready
    }
    let tim6 = &perip.TIM6;
    tim6.cr1.modify(|_, w| w.cen().set_bit());

    // Start ADC
    adc.cr.modify(|_, w| w.adstart().start()); // ADC start
}

pub struct LocalClock {}

impl dynamixel_f_rs::Clock for LocalClock {
    fn get_current_time(&self) -> Duration {
        Duration::from_micros(0)
    }
}

impl LocalClock {
    pub fn new() -> Self {
        Self {}
    }

    pub fn init(&self) {}
}

// For RS485
pub struct Uart1 {
    pub buffer_ : dynamixel_f_rs::RingBuffer<128>,
}

impl dynamixel_f_rs::BufferInterface for Uart1 {
    fn write_byte(&mut self, data: u8) {
        self.putc(data);
    }
    fn write_bytes(&mut self, data: &[u8]) {
        for d in data {
            self.write_byte(*d);
        }
        // for d in data { defmt::info!("write 0x{:x}", d); }
    }
    fn read_byte(&mut self) -> Option<u8> {
        self.buffer_.dequeue()
    }
    fn read_bytes(&mut self, buf: &mut [u8]) -> Option<usize> {
        if self.buffer_.is_empty() {
            return None;
        }
        for i in 0..buf.len() {
            match self.buffer_.dequeue() {
                Some(v) => {buf[i] = v},
                None => {return Some(i)},
            }
        }
        Some(buf.len())
    }
    fn clear_read_buf(&mut self) {}
}

// データを流し込む用
impl dynamixel_f_rs::QueueInterface for Uart1 {
    fn enqueue(&mut self, data: u8) -> Result<(), ()> {
        self.buffer_.enqueue(data)
    }
}

impl Uart1 {
    pub fn new() -> Self {
        Self {
            buffer_ : dynamixel_f_rs::RingBuffer::new(),
        }
    }

    pub fn init(&self) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                // GPIOポートの電源投入(クロックの有効化)
                perip.RCC.ahb2enr.modify(|_, w| w.gpioaen().set_bit());

                perip.RCC.apb2enr.modify(|_, w| w.usart1en().enabled());

                // gpioモード変更
                let gpio = &perip.GPIOA;
                gpio.moder.modify(|_, w| w.moder9().alternate());
                gpio.moder.modify(|_, w| w.moder10().alternate());
                gpio.moder.modify(|_, w| w.moder12().alternate());
                gpio.afrh.modify(|_, w| w.afrh9().af7());
                gpio.afrh.modify(|_, w| w.afrh10().af7());
                gpio.afrh.modify(|_, w| w.afrh12().af7());

                let uart = &perip.USART1;
                // Set over sampling mode
                uart.cr1.modify(|_, w| w.over8().clear_bit());
                // Set parity mode
                uart.cr1.modify(|_, w| w.pce().clear_bit());
                // Set word length
                uart.cr1.modify(|_, w| w.m0().clear_bit());
                uart.cr1.modify(|_, w| w.m1().clear_bit());
                // FIFO enable
                uart.cr1.modify(|_, w| w.fifoen().set_bit());

                // FIFO empty interrupt is generated if RXFNEIE = 1 in the USART_CR1 register
                uart.cr1.modify(|_, w| w.rxneie().set_bit());
                
                // Set baud rate
                uart.brr.modify(|_, w| unsafe { w.bits(0x4BF) }); // 140MHz / 115200

                // Set stop bit
                uart.cr2.modify(|_, w| unsafe { w.stop().bits(0b00) });

                // RS485 driver enable
                uart.cr3.modify(|_, w| w.dem().set_bit());

                // Set uart enable
                uart.cr1.modify(|_, w| w.ue().set_bit());

                // Set uart recieve enable
                uart.cr1.modify(|_, w| w.re().set_bit());
                // Set uart transmitter enable
                uart.cr1.modify(|_, w| w.te().set_bit());
            }
        });
    }
    fn putc(&self, c: u8) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                let uart = &perip.USART1;
                uart.tdr.modify(|_, w| unsafe { w.tdr().bits(c.into()) });
                // while uart.isr.read().tc().bit_is_set() {}
                while uart.isr.read().txe().bit_is_clear() {}
            }
        });
    }
}

pub struct Uart3 {}
impl Write for Uart3 {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.bytes() {
            self.putc(c);
        }
        Ok(())
    }
}

impl Uart3 {
    pub fn new() -> Self {
        Self {}
    }

    pub fn init(&self) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                // GPIOポートの電源投入(クロックの有効化)
                perip.RCC.ahb2enr.modify(|_, w| w.gpioben().set_bit());

                perip.RCC.apb1enr1.modify(|_, w| w.usart3en().enabled());

                // gpioモード変更
                let gpiob = &perip.GPIOB;
                gpiob.moder.modify(|_, w| w.moder8().alternate());
                gpiob.moder.modify(|_, w| w.moder9().alternate());
                gpiob.afrh.modify(|_, w| w.afrh8().af7());
                gpiob.afrh.modify(|_, w| w.afrh9().af7());
                // ここまでみた
                let uart = &perip.USART3;
                // Set over sampling mode
                uart.cr1.modify(|_, w| w.over8().clear_bit());
                // Set parity mode
                uart.cr1.modify(|_, w| w.pce().clear_bit());
                // Set word length
                uart.cr1.modify(|_, w| w.m0().clear_bit());
                uart.cr1.modify(|_, w| w.m1().clear_bit());
                // FIFO?
                // Set baud rate
                uart.brr.modify(|_, w| unsafe { w.bits(0x4BF) }); // 140MHz / 115200

                // Set stop bit
                uart.cr2.modify(|_, w| unsafe { w.stop().bits(0b00) });
                // Set swap
                uart.cr2.modify(|_, w| w.swap().set_bit());

                // Set uart enable
                uart.cr1.modify(|_, w| w.ue().set_bit());

                // Set uart recieve enable
                uart.cr1.modify(|_, w| w.re().set_bit());
                // Set uart transmitter enable
                uart.cr1.modify(|_, w| w.te().set_bit());
            }
        });
    }
    fn putc(&self, c: u8) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                let uart = &perip.USART3;
                uart.tdr.modify(|_, w| unsafe { w.tdr().bits(c.into()) });
                // while uart.isr.read().tc().bit_is_set() {}
                while uart.isr.read().txe().bit_is_clear() {}
            }
        });
    }
}

pub struct SPI2 {}

impl SPI2 {
    pub fn new() -> Self {
        Self {}
    }

    pub fn init(&self) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                // GPIOポートの電源投入(クロックの有効化)
                perip.RCC.ahb2enr.modify(|_, w| w.gpioben().set_bit());

                perip.RCC.apb1enr1.modify(|_, w| w.spi2en().enabled());

                // gpioモード変更
                let gpiob = &perip.GPIOB;
                // gpiob.moder.modify(|_, w| w.moder12().alternate());  // CS pin
                gpiob.moder.modify(|_, w| w.moder12().output());
                gpiob.moder.modify(|_, w| w.moder13().alternate());
                gpiob.moder.modify(|_, w| w.moder14().alternate());
                gpiob.moder.modify(|_, w| w.moder15().alternate());
                // gpiob.afrh.modify(|_, w| w.afrh12().af5());  // CS pin
                gpiob.afrh.modify(|_, w| w.afrh13().af5());
                gpiob.afrh.modify(|_, w| w.afrh14().af5());
                gpiob.afrh.modify(|_, w| w.afrh15().af5());
                gpiob.ospeedr.modify(|_, w| w.ospeedr12().very_high_speed()); // CS pin
                gpiob.ospeedr.modify(|_, w| w.ospeedr13().very_high_speed());
                gpiob.ospeedr.modify(|_, w| w.ospeedr14().very_high_speed());
                gpiob.ospeedr.modify(|_, w| w.ospeedr15().very_high_speed());
                gpiob.otyper.modify(|_, w| w.ot12().push_pull()); // CS pin
                gpiob.otyper.modify(|_, w| w.ot13().push_pull());
                gpiob.otyper.modify(|_, w| w.ot14().push_pull());
                gpiob.otyper.modify(|_, w| w.ot15().push_pull());

                let spi = &perip.SPI2;
                spi.cr1.modify(|_, w| w.spe().clear_bit());

                // Set Baudrate
                spi.cr1.modify(|_, w| unsafe { w.br().bits(0b0111) }); // f_pclk / 256

                // Set Clock polarity
                spi.cr1.modify(|_, w| w.cpol().set_bit()); // idle high

                // Set Clock phase
                spi.cr1.modify(|_, w| w.cpha().set_bit()); // second edge(rising edge in-case idle is high)

                // Bidirectional data mode enable(half-duplex communication)
                spi.cr1.modify(|_, w| w.bidimode().clear_bit());
                // Set MSL LSB first
                spi.cr1.modify(|_, w| w.lsbfirst().clear_bit());
                // Set NSS management
                // Soft ware slave management
                spi.cr1.modify(|_, w| w.ssm().set_bit());
                // Internal slave select
                spi.cr1.modify(|_, w| w.ssi().set_bit());
                // Master configuration
                spi.cr1.modify(|_, w| w.mstr().set_bit());

                // Data size
                spi.cr2.modify(|_, w| unsafe { w.ds().bits(0b0111) }); // 8bit

                // SS output
                spi.cr2.modify(|_, w| w.ssoe().clear_bit());
                // Frame format
                spi.cr2.modify(|_, w| w.frf().clear_bit()); // Motorola mode

                // NSS pulse management
                spi.cr2.modify(|_, w| w.nssp().set_bit());
                //
                spi.cr1.modify(|_, w| w.spe().set_bit());
            }
        });
    }
    pub fn txrx(&self, c: u16) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                let gpiob = &perip.GPIOB;
                gpiob.bsrr.write(|w| w.br12().reset());
                let spi = &perip.SPI2;

                while spi.sr.read().txe().bit_is_clear() {}
                // send 8bit data automatically 2 times
                spi.dr.modify(|_, w| unsafe { w.dr().bits(c.into()) });

                while spi.sr.read().bsy().bit_is_set() {}
                while spi.sr.read().rxne().bit_is_clear() {}
                gpiob.bsrr.write(|w| w.bs12().set());
                // defmt::info!("dr: {:x}", spi.dr.read().dr().bits());
            }
        });
    }
}

pub struct Fsr0 {}

impl Fsr for Fsr0 {
    fn get_force(&self) -> f32 {
        0.0
    }
}

impl Fsr0 {
    pub fn new() -> Self {
        Self {}
    }
    pub fn sigle_conversion(&self) -> u16 {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => 0, //TODO change to some
            Some(perip) => {
                let adc = &perip.ADC2;
                adc.cr.modify(|_, w| w.adstart().start()); // ADC start
                while adc.isr.read().eoc().is_not_complete() {
                    // Wait for ADC complete
                }
                adc.isr.modify(|_, w| w.eoc().clear()); // clear eoc flag

                adc.dr.read().rdata().bits()
            }
        })
    }
}

pub struct Led0 {}

impl Indicator for Led0 {
    fn on(&self) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                let gpioc = &perip.GPIOC;
                gpioc.bsrr.write(|w| w.bs13().set());
            }
        });
    }
    fn off(&self) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                let gpioc = &perip.GPIOC;
                gpioc.bsrr.write(|w| w.br13().reset());
            }
        });
    }
    fn toggle(&self) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                let gpioc = &perip.GPIOC;
                if gpioc.odr.read().odr13().is_low() {
                    gpioc.bsrr.write(|w| w.bs13().set());
                } else {
                    gpioc.bsrr.write(|w| w.br13().reset());
                }
            }
        });
    }
}

impl Led0 {
    pub fn new() -> Self {
        Self {}
    }

    pub fn init(&self) {
        free(|cs| {
            match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
                None => (),
                Some(perip) => {
                    // GPIOポートの電源投入(クロックの有効化)
                    perip.RCC.ahb2enr.modify(|_, w| w.gpiocen().set_bit());

                    // gpioモード変更
                    let gpioc = &perip.GPIOC;
                    gpioc.moder.modify(|_, w| w.moder13().output());
                }
            }
        });
    }
}

pub struct Led1 {}

impl Indicator for Led1 {
    fn on(&self) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                let gpioc = &perip.GPIOC;
                gpioc.bsrr.write(|w| w.bs14().set());
            }
        });
    }
    fn off(&self) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                let gpioc = &perip.GPIOC;
                gpioc.bsrr.write(|w| w.br14().reset());
            }
        });
    }
    fn toggle(&self) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                let gpioc = &perip.GPIOC;
                if gpioc.odr.read().odr14().is_low() {
                    gpioc.bsrr.write(|w| w.bs14().set());
                } else {
                    gpioc.bsrr.write(|w| w.br14().reset());
                }
            }
        });
    }
}

impl Led1 {
    pub fn new() -> Self {
        Self {}
    }

    pub fn init(&self) {
        free(|cs| {
            match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
                None => (),
                Some(perip) => {
                    // GPIOポートの電源投入(クロックの有効化)
                    perip.RCC.ahb2enr.modify(|_, w| w.gpiocen().set_bit());

                    // gpioモード変更
                    let gpioc = &perip.GPIOC;
                    gpioc.moder.modify(|_, w| w.moder14().output());
                }
            }
        });
    }
}

pub struct Led2 {}

impl Indicator for Led2 {
    fn on(&self) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                let gpioc = &perip.GPIOC;
                gpioc.bsrr.write(|w| w.bs15().set());
            }
        });
    }
    fn off(&self) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                let gpioc = &perip.GPIOC;
                gpioc.bsrr.write(|w| w.br15().reset());
            }
        });
    }
    fn toggle(&self) {
        free(|cs| match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
            None => (),
            Some(perip) => {
                let gpioc = &perip.GPIOC;
                if gpioc.odr.read().odr15().is_low() {
                    gpioc.bsrr.write(|w| w.bs15().set());
                } else {
                    gpioc.bsrr.write(|w| w.br15().reset());
                }
            }
        });
    }
}

impl Led2 {
    pub fn new() -> Self {
        Self {}
    }

    pub fn init(&self) {
        free(|cs| {
            match G_PERIPHERAL.borrow(cs).borrow().as_ref() {
                None => (),
                Some(perip) => {
                    // GPIOポートの電源投入(クロックの有効化)
                    perip.RCC.ahb2enr.modify(|_, w| w.gpiocen().set_bit());

                    // gpioモード変更
                    let gpioc = &perip.GPIOC;
                    gpioc.moder.modify(|_, w| w.moder15().output());
                }
            }
        });
    }
}
