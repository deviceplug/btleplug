use ::adapter::BDAddr;

#[derive(Debug, Clone)]
pub struct Device {
    pub address: BDAddr,
    pub local_name: Option<String>,
    pub tx_power_level: Option<i8>,
    pub manufacturer_data: Option<Vec<u8>>,
    pub discovery_count: u32,
    pub has_scan_response: bool,

    // TODO service_data, service_uuids, solicitation_uuids
}

impl Device {
    pub fn new(address: BDAddr) -> Device {
        Device {
            address,
            local_name: None,
            tx_power_level: None,
            manufacturer_data: None,
            discovery_count: 0,
            has_scan_response: false,
        }
    }
}
