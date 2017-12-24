use ::adapter::BDAddr;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum AddressType {
    Random,
    Public,
}

impl AddressType {
    pub fn num(&self) -> u8 {
        match *self {
            AddressType::Public => 0,
            AddressType::Random => 1
        }
    }
}

#[derive(Debug, Clone)]
pub struct Device {
    pub address: BDAddr,
    pub address_type: AddressType,
    pub local_name: Option<String>,
    pub tx_power_level: Option<i8>,
    pub manufacturer_data: Option<Vec<u8>>,
    pub discovery_count: u32,
    pub has_scan_response: bool,

    // TODO service_data, service_uuids, solicitation_uuids
}

impl Device {
    pub fn new(address: BDAddr, address_type: AddressType) -> Device {
        Device {
            address,
            address_type,
            local_name: None,
            tx_power_level: None,
            manufacturer_data: None,
            discovery_count: 0,
            has_scan_response: false,
        }
    }
}
