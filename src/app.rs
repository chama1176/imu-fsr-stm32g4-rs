use crate::{imu_fsr_stm32g4::Uart3, imu_fsr_stm32g4::SPI2, indicator::Indicator};

pub struct App<T0, T1, T2>
where
    T0: Indicator,
    T1: Indicator,
    T2: Indicator,
{
    led0: T0,
    led1: T1,
    led2: T2,
    uart: Uart3, // TODO: interfaceを整理
    spi: SPI2,   // TODO: interfaceを整理
}

impl<T0, T1, T2> App<T0, T1, T2>
where
    T0: Indicator,
    T1: Indicator,
    T2: Indicator,
{
    pub fn new(led0: T0, led1: T1, led2: T2, uart: Uart3, spi: SPI2) -> Self {
        Self {
            led0,
            led1,
            led2,
            uart,
            spi,
        }
    }
    pub fn periodic_task(&self) {
        self.led0.toggle();
        self.led1.toggle();
        self.led2.toggle();
    }
    pub fn read_imu_task(&self) {
        self.spi.txrx(0x1F1F | 0b0000_0000); // enable
        self.spi.txrx(0x75 | 0b1000_0000); // who am i
        self.spi.txrx(0x0F | 0b1000_0000); // accel z
        self.spi.txrx(0x10 | 0b1000_0000); // accel z
    }
    pub fn update_fsr_task(&self) {}
    pub fn parse_uart_task(&self) {}
}
