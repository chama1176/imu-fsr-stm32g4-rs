use dynamixel_f_rs::control_table::BitsW;

use crate::{imu_fsr_stm32g4::Uart3, imu_fsr_stm32g4::SPI2, indicator::Indicator};

pub struct App<T0, T1, T2, I, C>
where
    T0: Indicator,
    T1: Indicator,
    T2: Indicator,
    I: dynamixel_f_rs::BufferInterface+dynamixel_f_rs::QueueInterface,
    C: dynamixel_f_rs::Clock,
{
    led0: T0,
    led1: T1,
    led2: T2,
    uart: Uart3, // TODO: interfaceを整理
    spi: SPI2,   // TODO: interfaceを整理
    dxl: dynamixel_f_rs::DynamixelProtocolHandler<I, C>,
}

impl<T0, T1, T2, I, C> App<T0, T1, T2, I, C>
where
    T0: Indicator,
    T1: Indicator,
    T2: Indicator,
    I: dynamixel_f_rs::BufferInterface+dynamixel_f_rs::QueueInterface,
    C: dynamixel_f_rs::Clock,
{
    pub fn new(
        led0: T0,
        led1: T1,
        led2: T2,
        uart: Uart3,
        spi: SPI2,
        mut buffer_interface: I,
        clock: C,
    ) -> Self {
        let ctd = dynamixel_f_rs::ControlTableData::new();
        let dxl =
            dynamixel_f_rs::DynamixelProtocolHandler::new(buffer_interface, clock, 115200, ctd);
        Self {
            led0,
            led1,
            led2,
            uart,
            spi,
            dxl,
        }
    }
    pub fn periodic_task(&self) {
        self.led0.toggle();
        self.led1.toggle();
        self.led2.toggle();
        defmt::info!("goal position: {}", self.dxl.ctd.read().goal_position());

    }
    pub fn read_imu_task(&self) {
        self.spi.txrx(0x1F1F | 0b0000_0000); // enable
        self.spi.txrx(0x75 | 0b1000_0000); // who am i
        self.spi.txrx(0x0F | 0b1000_0000); // accel z
        self.spi.txrx(0x10 | 0b1000_0000); // accel z
    }
    pub fn update_fsr_task(&self) {
        // ctdの編集
        self.dxl.ctd.modify(|_, w| w.led().bits(1));
    }
    pub fn enque_uart(&mut self, data: u8) {
        self.dxl.uart.enqueue(data).unwrap();
    }
    pub fn init(&self){
        self.dxl.ctd.modify(|_, w| w.id().bits(1));
        self.dxl.ctd.modify(|_, w| w.present_position().bits(777));
        

        defmt::info!("id: {}", self.dxl.ctd.read().id());
        defmt::info!("present position: {}", self.dxl.ctd.read().present_position());

    }
    pub fn parse_uart_task(&mut self) {
        // Dxl処理(受信があった場合自動返信するはず)
        let r = self.dxl.parse_data();
        match r {
            Ok(_) => {
                defmt::info!("ok");
            }
            Err(e) => {
                defmt::info!("error");
            }
        }
    }
}
