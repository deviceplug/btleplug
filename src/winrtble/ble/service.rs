use super::characteristic::BLECharacteristic;
use crate::api::Service;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug)]
pub struct BLEService {
    pub uuid: Uuid,
    pub characteristics: HashMap<Uuid, BLECharacteristic>,
}

impl BLEService {
    pub fn to_service(&self) -> Service {
        let characteristics = self
            .characteristics
            .values()
            .map(|ble_characteristic| ble_characteristic.to_characteristic(self.uuid))
            .collect();
        Service {
            uuid: self.uuid,
            primary: true,
            characteristics,
        }
    }
}
