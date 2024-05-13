use crate::indicator::Indicator;

pub struct App<T0, T1, T2>
where
    T0: Indicator,
    T1: Indicator,
    T2: Indicator,
{
    led0: T0,
    led1: T1,
    led2: T2,
}

impl<T0, T1, T2> App<T0, T1, T2>
where
    T0: Indicator,
    T1: Indicator,
    T2: Indicator,
{
    pub fn new(led0: T0, led1: T1, led2: T2) -> Self {
        Self { led0, led1, led2 }
    }
    pub fn periodic_task(&self) {
        self.led0.toggle();
        self.led1.toggle();
        self.led2.toggle();
    }
}
