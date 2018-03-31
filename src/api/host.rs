use nix;
use device::Characteristic;
use adapter::EventHandler;
use api::BDAddr;
use api::HandleFn;
use api::peripheral::Peripheral;

pub trait Host {
    fn watch(&self, handler: EventHandler);

    fn start_scan(&self) -> nix::Result<()>;
    fn stop_scan(&self) -> nix::Result<()>;

    fn peripherals(&self) -> Vec<Box<Peripheral>>;
    fn peripheral(&self, address: BDAddr) -> Option<Box<Peripheral>>;
}
