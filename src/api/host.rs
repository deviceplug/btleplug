use api::BDAddr;
use api::peripheral::Peripheral;
use ::Result;

#[derive(Debug, Copy, Clone)]
pub enum Event {
    DeviceDiscovered(BDAddr),
    DeviceLost(BDAddr),
    DeviceUpdated(BDAddr),
    DeviceConnected(BDAddr),
    DeviceDisconnected(BDAddr),
}

pub type EventHandler = Box<Fn(Event) + Send>;

pub trait Host {
    fn on_event(&self, handler: EventHandler);

    fn start_scan(&self) -> Result<()>;
    fn stop_scan(&self) -> Result<()>;

    fn peripherals(&self) -> Vec<Box<Peripheral>>;
    fn peripheral(&self, address: BDAddr) -> Option<Box<Peripheral>>;
}
