use ::Result;
use ::Error;
use winrt::ComPtr;
use winrt::windows::devices::bluetooth::genericattributeprofile::{GattCommunicationStatus, GattCharacteristic, GattValueChangedEventArgs, GattClientCharacteristicConfigurationDescriptorValue};
use winrt::windows::storage::streams::DataReader;
use winrt::RtAsyncOperation;
use winrt::windows::foundation::{ TypedEventHandler, EventRegistrationToken };

pub type NotifiyEventHandler = Box<Fn(Vec<u8>) + Send>;

pub struct BLECharacteristic {
    characteristic: ComPtr<GattCharacteristic>,
    notify_token: Option<EventRegistrationToken>,
}

unsafe impl Send for BLECharacteristic {}
unsafe impl Sync for BLECharacteristic {}

impl BLECharacteristic {
    pub fn new(characteristic: ComPtr<GattCharacteristic>) -> Self {
        BLECharacteristic { characteristic, notify_token: None }
    }

    pub fn read_value(&self) -> Result<Vec<u8>> {
        let result = self.characteristic.read_value_async().unwrap().blocking_get().unwrap().unwrap();
        if result.get_status().unwrap() == GattCommunicationStatus::Success {
            let value = result.get_value().unwrap().unwrap();
            let reader = DataReader::from_buffer(&value).unwrap().unwrap();
            let len = reader.get_unconsumed_buffer_length().unwrap() as usize;
            let mut input = vec![0u8; len];
            reader.read_bytes(&mut input[0..len]).unwrap();
            Ok(input)
        } else {
            Err(Error::NotSupported("get_status".into()))
        }
    }

    pub fn subscribe(&mut self, on_value_changed: NotifiyEventHandler) -> Result<()> {
        let value_handler = TypedEventHandler::new(move |_: *mut GattCharacteristic, args: *mut GattValueChangedEventArgs| {
            let args = unsafe { (&*args) };
            let value = args.get_characteristic_value().unwrap().unwrap();
            let reader = DataReader::from_buffer(&value).unwrap().unwrap();
            let len = reader.get_unconsumed_buffer_length().unwrap() as usize;
            let mut input = vec![0u8; len];
            reader.read_bytes(&mut input[0..len]).unwrap();
            println!("changed {:?}", input);
            on_value_changed(input);
            Ok(())
        });
        let token = self.characteristic.add_value_changed(&value_handler).unwrap();
        self.notify_token = Some(token);
        let config = GattClientCharacteristicConfigurationDescriptorValue::Indicate;
        let status = self.characteristic.write_client_characteristic_configuration_descriptor_async(config).unwrap().blocking_get().unwrap();
        println!("{:?}", status);
        Ok(())
    }

    pub fn unsubscribe(&mut self) -> Result<()> {
        if let Some(token) = self.notify_token {
            self.characteristic.remove_value_changed(token).unwrap();
        }
        self.notify_token = None;
        let config = GattClientCharacteristicConfigurationDescriptorValue::None;
        let status = self.characteristic.write_client_characteristic_configuration_descriptor_async(config).unwrap().blocking_get().unwrap();
        println!("{:?}", status);
        Ok(())
    }
}

impl Drop for BLECharacteristic {
    fn drop(&mut self) {
        if let Some(token) = self.notify_token {
            let result = self.characteristic.remove_value_changed(token);
            if let Err(err) = result {
                println!("Drop:remove_connection_status_changed {:?}", err);
            }
        }
    }
}
