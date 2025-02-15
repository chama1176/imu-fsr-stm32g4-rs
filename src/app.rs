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
        // self.led2.toggle();
        // defmt::info!("goal position: {}", self.dxl.ctd.read().goal_position());
        // defmt::info!("goal current: {}", self.dxl.ctd.read().goal_current());

    }
    pub fn update_task(&self) {
        self.update_fsr_task();
        self.read_imu_task();

        if self.dxl.ctd.read().led() == 1 {
            self.led2.on();
        } else {
            self.led2.off();
        }


    }
    fn read_imu_task(&self) {
        // defmt::warn!("read imu task");
        self.spi.txrx(0x1F1F | 0b0000_0000).unwrap(); // enable
        self.spi.txrx(0x75 | 0b1000_0000).unwrap(); // who am i
        
        let accel_z_upper =  self.spi.txrx(0x0F | 0b1000_0000).unwrap(); // accel z
        let accel_z_lower =  self.spi.txrx(0x10 | 0b1000_0000).unwrap(); // accel z
        let accel_z_raw = (((accel_z_upper as u16) << 8) | accel_z_lower as u16) as i16;
        let accel_z = accel_z_raw as f32 * 16.0 / 32767.0;
        defmt::info!("accel z: {}", accel_z);
        self.dxl.ctd.modify(|_, w| w.present_position().bits(accel_z_raw as i32));
    }
    fn update_fsr_task(&self) {
        // ctdの編集
        // self.dxl.ctd.modify(|_, w| w.led().bits(1));
    }
    pub fn enque_uart(&mut self, data: u8) {
        self.dxl.uart.enqueue(data).unwrap();
    }
    pub fn init(&self){
        // ctdの初期値は全部0
        self.dxl.ctd.modify(|_, w| w.id().bits(1));
        
        self.dxl.ctd.modify(|_, w| w.model_number().bits(0x04BA)); // 0x04BA: XC330-T181-T
        self.dxl.ctd.modify(|_, w| w.firmware_version().bits(0x30));
        self.dxl.ctd.modify(|_, w| w.baud_rate().bits(0x06)); // 0x06: 4Mbps
        self.dxl.ctd.modify(|_, w| w.return_delay_time().bits(0x00));
        self.dxl.ctd.modify(|_, w| w.drive_mode().bits(0x00));
        self.dxl.ctd.modify(|_, w| w.operating_mode().bits(0x00)); // 0x00 current control
        self.dxl.ctd.modify(|_, w| w.moving_threshold().bits(0x0A)); // default
        self.dxl.ctd.modify(|_, w| w.temperature_limit().bits(0x46)); // 70℃
        self.dxl.ctd.modify(|_, w| w.max_voltage_limit().bits(0x8C)); // 14V
        self.dxl.ctd.modify(|_, w| w.min_voltage_limit().bits(0x37)); // 5.5V

        self.dxl.ctd.modify(|_, w| w.present_position().bits(-777));

    }
    pub fn parse_uart_task(&mut self) {
        // Dxl処理(受信があった場合自動返信するはず)
        let r = self.dxl.parse_data();
        match r {
            Ok(_) => {
            }
            Err(e) => {
                defmt::info!("error");
            }
        }
    }
}
