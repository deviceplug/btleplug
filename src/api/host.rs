use adapter::EventHandler;
use api::BDAddr;
use api::peripheral::Peripheral;
use ::Result;

pub trait Host {
    fn watch(&self, handler: EventHandler);

    fn start_scan(&self) -> Result<()>;
    fn stop_scan(&self) -> Result<()>;

    fn peripherals(&self) -> Vec<Box<Peripheral>>;
    fn peripheral(&self, address: BDAddr) -> Option<Box<Peripheral>>;
}
