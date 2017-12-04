use ::adapter::BDAddr;

#[derive(Debug, Clone)]
pub struct Device {
    pub addr: BDAddr,
    pub name: Option<String>,
}

pub struct ConnectedDevice {
}


impl Device {
    pub fn connect() -> ConnectedDevice {
        unimplemented!()
    }
}

