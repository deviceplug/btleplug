use winrt::RuntimeContext;
use winrt::RtAsyncOperation;
use winrt::windows::devices::radios::{Radio, RadioKind};
use winrtble::adapter::Adapter;
use ::Result;
use ::Error;

pub struct Manager {
    _rt: RuntimeContext,
}

impl Manager {
    pub fn new() -> Manager {
        let _rt = RuntimeContext::init();
        Manager { _rt }
    }

    pub fn adapters(&self) -> Result<Adapter> {
        let radios = Radio::get_radios_async().unwrap().blocking_get().unwrap().unwrap();

        for radio in &radios {
            if let Some(radio) = radio {
                if let Ok(kind) = radio.get_kind() {
                    if kind == RadioKind::Bluetooth {
                        return Ok(Adapter::new());
                    }
                }
            }
        }
        Err(Error::NotSupported("no bluetooth adapter found".into()))
    }
}
