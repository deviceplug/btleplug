use ::adapter::BDAddr;

#[derive(Debug)]
pub struct Device {
    pub addr: BDAddr,
    pub name: Option<String>,
}

