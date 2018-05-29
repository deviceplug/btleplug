use winrt::ComPtr;
use winrt::windows::devices::bluetooth::advertisement::*;
use winrt::windows::foundation::{TypedEventHandler};
use winrt::RtDefaultConstructible;
use ::Result;

pub type AdvertismentEventHandler = Box<Fn(&BluetoothLEAdvertisementReceivedEventArgs) + Send>;

pub struct BLEWatcher {
    watcher: ComPtr<BluetoothLEAdvertisementWatcher>,
}

unsafe impl Send for BLEWatcher {}
unsafe impl Sync for BLEWatcher {}

impl BLEWatcher {
    pub fn new() -> Self {
        let ad = BluetoothLEAdvertisementFilter::new();
        let watcher = BluetoothLEAdvertisementWatcher::create(&ad).unwrap();
        BLEWatcher{ watcher }
    }

    pub fn start(&self, on_received: AdvertismentEventHandler) -> Result<()> {
        self.watcher.set_scanning_mode(BluetoothLEScanningMode::Active).unwrap();
        let handler = TypedEventHandler::new(move |_sender, args: *mut BluetoothLEAdvertisementReceivedEventArgs| {
            let args = unsafe { (&*args) }; 
            on_received(args);
            Ok(())
        });
        self.watcher.add_received(&handler).unwrap();
        self.watcher.start().unwrap();
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        self.watcher.stop().unwrap();
        Ok(())
    }
}
