use super::utils::{uuid_from_string, wrap_promise};
use crate::api::{
    self, BDAddr, CentralEvent, CharPropFlags, Characteristic, PeripheralProperties, Service,
    ValueNotification, WriteType,
};
use crate::common::{
    adapter_manager::AdapterManager, util::notifications_stream_from_broadcast_receiver,
};
use crate::{Error, Result};
use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::stream::{Stream, StreamExt};
use js_sys::{Array, DataView, Uint8Array};
use std::collections::{BTreeSet, HashMap};
use std::fmt::{self, Debug, Formatter};
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::broadcast;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{
    BluetoothCharacteristicProperties, BluetoothDevice, BluetoothRemoteGattCharacteristic,
    BluetoothRemoteGattServer, BluetoothRemoteGattService, Event,
};

macro_rules! send_cmd {
    ($self:ident, $cmd:ident$(, $opt:expr)*) => {{
        let (sender, receiver) = oneshot::channel();
        let _ = $self.shared.sender.unbounded_send(PeripheralSharedCmd::$cmd(sender, $($opt),*));
        receiver.await.unwrap()
    }};
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PeripheralId(String);

/// Implementation of [api::Peripheral](crate::api::Peripheral).
#[derive(Clone)]
pub struct Peripheral {
    shared: Arc<Shared>,
}

enum PeripheralSharedCmd {
    IsConnected(oneshot::Sender<Result<bool>>),
    Connect(oneshot::Sender<Result<()>>),
    Disconnect(oneshot::Sender<Result<()>>),
    DiscoverServices(oneshot::Sender<Result<BTreeSet<Service>>>),
    Read(oneshot::Sender<Result<Vec<u8>>>, Uuid),
    Write(oneshot::Sender<Result<()>>, Uuid, Vec<u8>, WriteType),
    Subscribe(oneshot::Sender<Result<()>>, Uuid),
    Unsubscribe(oneshot::Sender<Result<()>>, Uuid),
}

struct Shared {
    id: String,
    name: Option<String>,
    services: Mutex<BTreeSet<Service>>,
    sender: mpsc::UnboundedSender<PeripheralSharedCmd>,
    notifications_channel: broadcast::Sender<ValueNotification>,
}

struct SharedExecuter {
    manager: Weak<AdapterManager<Peripheral>>,
    device: BluetoothDevice,
    characteristics: HashMap<Uuid, BluetoothRemoteGattCharacteristic>,
    ongattserverdisconnected: Closure<dyn FnMut(Event)>,
    oncharacteristicvaluechanged: Closure<dyn FnMut(Event)>,
}

impl SharedExecuter {
    fn gatt(&self) -> BluetoothRemoteGattServer {
        self.device.gatt().unwrap()
    }

    async fn is_connected(&self) -> Result<bool> {
        Ok(self.gatt().connected())
    }

    async fn connect(&self) -> Result<()> {
        if self.gatt().connected() {
            return Ok(());
        }

        wrap_promise::<BluetoothRemoteGattServer>(self.gatt().connect())
            .await
            .map(|gatt| {
                if let Some(manager) = self.manager.upgrade() {
                    manager.emit(CentralEvent::DeviceConnected(gatt.device().id().into()));
                }
                ()
            })
    }

    async fn disconnect(&self) -> Result<()> {
        Ok(self.gatt().disconnect())
    }

    async fn discover_services(&mut self) -> Result<BTreeSet<Service>> {
        self.characteristics.clear();
        let services = wrap_promise::<Array>(self.gatt().get_primary_services()).await?;
        let mut ret = BTreeSet::new();
        for service in services.iter() {
            let mut characteristics = BTreeSet::new();
            let service = BluetoothRemoteGattService::from(service);
            let service_uuid = uuid_from_string(service.uuid());

            if let Ok(chars) = wrap_promise::<Array>(service.get_characteristics()).await {
                for ch in chars.iter() {
                    let ch = BluetoothRemoteGattCharacteristic::from(ch);
                    let uuid = uuid_from_string(ch.uuid());
                    characteristics.insert(Characteristic {
                        uuid,
                        service_uuid,
                        properties: ch.properties().into(),
                    });
                    self.characteristics.insert(uuid, ch);
                }
            }

            ret.insert(Service {
                uuid: service_uuid,
                primary: service.is_primary(),
                characteristics,
            });
        }
        Ok(ret)
    }

    fn get_characteristic(&self, uuid: Uuid) -> Result<&BluetoothRemoteGattCharacteristic> {
        self.characteristics.get(&uuid).map_or(
            Err(Error::NotSupported("Characteristic not found".into())),
            |characteristic| Ok(characteristic),
        )
    }

    async fn write(&self, uuid: Uuid, mut data: Vec<u8>, write_type: WriteType) -> Result<()> {
        let characteristic = self.get_characteristic(uuid)?;
        wrap_promise::<JsValue>(match write_type {
            WriteType::WithResponse => {
                characteristic.write_value_with_response_with_u8_array(&mut data)
            }
            WriteType::WithoutResponse => {
                characteristic.write_value_without_response_with_u8_array(&mut data)
            }
        })
        .await
        .map(|_| ())
    }

    async fn read(&self, uuid: Uuid) -> Result<Vec<u8>> {
        let characteristic = self.get_characteristic(uuid)?;
        wrap_promise::<DataView>(characteristic.read_value())
            .await
            .map(|value| Uint8Array::new(&value.buffer()).to_vec())
    }

    async fn subscribe(&self, uuid: Uuid) -> Result<()> {
        let characteristic = self.get_characteristic(uuid)?;
        characteristic.set_oncharacteristicvaluechanged(Some(
            self.oncharacteristicvaluechanged.as_ref().unchecked_ref(),
        ));
        wrap_promise::<JsValue>(characteristic.start_notifications())
            .await
            .map(|_| ())
    }

    async fn unsubscribe(&self, uuid: Uuid) -> Result<()> {
        let characteristic = self.get_characteristic(uuid)?;
        characteristic.set_oncharacteristicvaluechanged(None);
        wrap_promise::<JsValue>(characteristic.stop_notifications())
            .await
            .map(|_| ())
    }

    fn new(
        manager: Weak<AdapterManager<Peripheral>>,
        device: BluetoothDevice,
        notifications_sender: broadcast::Sender<ValueNotification>,
    ) -> Self {
        let manager_clone = manager.clone();
        let ongattserverdisconnected = Closure::wrap(Box::new(move |e: Event| {
            let device = BluetoothDevice::from(JsValue::from(e.target().unwrap()));
            if let Some(manager_upgrade) = manager_clone.upgrade() {
                manager_upgrade.emit(CentralEvent::DeviceDisconnected(device.id().into()));
            }
        }) as Box<dyn FnMut(Event)>);

        let oncharacteristicvaluechanged = Closure::wrap(Box::new(move |e: Event| {
            let characteristic =
                BluetoothRemoteGattCharacteristic::from(JsValue::from(e.target().unwrap()));
            let notification = ValueNotification {
                uuid: uuid_from_string(characteristic.uuid()),
                value: characteristic
                    .value()
                    .map_or(vec![], |value| Uint8Array::new(&value.buffer()).to_vec()),
            };
            // Note: we ignore send errors here which may happen while there are no
            // receivers...
            let _ = notifications_sender.send(notification);
        }) as Box<dyn FnMut(Event)>);

        SharedExecuter {
            manager,
            device,
            characteristics: HashMap::new(),
            ongattserverdisconnected,
            oncharacteristicvaluechanged,
        }
    }

    async fn run(&mut self, mut receiver: mpsc::UnboundedReceiver<PeripheralSharedCmd>) {
        self.device.set_ongattserverdisconnected(Some(
            self.ongattserverdisconnected.as_ref().unchecked_ref(),
        ));

        while let Some(x) = receiver.next().await {
            match x {
                PeripheralSharedCmd::IsConnected(result) => {
                    let _ = result.send(self.is_connected().await);
                }
                PeripheralSharedCmd::Connect(result) => {
                    let _ = result.send(self.connect().await);
                }
                PeripheralSharedCmd::Disconnect(result) => {
                    let _ = result.send(self.disconnect().await);
                }
                PeripheralSharedCmd::DiscoverServices(result) => {
                    let _ = result.send(self.discover_services().await);
                }
                PeripheralSharedCmd::Read(result, characteristic) => {
                    let _ = result.send(self.read(characteristic).await);
                }
                PeripheralSharedCmd::Write(result, characteristic, data, write_type) => {
                    let _ = result.send(self.write(characteristic, data, write_type).await);
                }
                PeripheralSharedCmd::Subscribe(result, characteristic) => {
                    let _ = result.send(self.subscribe(characteristic).await);
                }
                PeripheralSharedCmd::Unsubscribe(result, characteristic) => {
                    let _ = result.send(self.unsubscribe(characteristic).await);
                }
            }
        }
    }
}

impl Shared {
    fn new(manager: Weak<AdapterManager<Peripheral>>, device: BluetoothDevice) -> Self {
        let id = device.id().clone();
        let name = device.name().clone();
        let services = Mutex::new(BTreeSet::<Service>::new());

        let (notifications_channel, _) = broadcast::channel(16);
        let mut shared_executer =
            SharedExecuter::new(manager.clone(), device, notifications_channel.clone());

        let (sender, receiver) = mpsc::unbounded();
        spawn_local(async move {
            shared_executer.run(receiver).await;
        });

        Self {
            id,
            name,
            services,
            sender,
            notifications_channel,
        }
    }
}

impl Peripheral {
    pub(crate) fn new(manager: Weak<AdapterManager<Self>>, device: BluetoothDevice) -> Self {
        Peripheral {
            shared: Arc::new(Shared::new(manager, device)),
        }
    }
}

#[async_trait]
impl api::Peripheral for Peripheral {
    fn id(&self) -> PeripheralId {
        self.shared.id.clone().into()
    }

    fn address(&self) -> BDAddr {
        BDAddr::default()
    }

    async fn properties(&self) -> Result<Option<PeripheralProperties>> {
        Ok(Some(PeripheralProperties {
            address: BDAddr::default(),
            address_type: None,
            local_name: self.shared.name.clone(),
            tx_power_level: None,
            rssi: None,
            manufacturer_data: HashMap::new(),
            service_data: HashMap::new(),
            services: Vec::new(),
        }))
    }

    fn services(&self) -> BTreeSet<Service> {
        self.shared.services.lock().unwrap().clone()
    }

    async fn is_connected(&self) -> Result<bool> {
        send_cmd!(self, IsConnected)
    }

    async fn connect(&self) -> Result<()> {
        send_cmd!(self, Connect)
    }

    async fn disconnect(&self) -> Result<()> {
        send_cmd!(self, Disconnect)
    }

    async fn discover_services(&self) -> Result<()> {
        send_cmd!(self, DiscoverServices).map(|services| {
            *self.shared.services.lock().unwrap() = services;
            ()
        })
    }

    async fn write(
        &self,
        characteristic: &Characteristic,
        data: &[u8],
        write_type: WriteType,
    ) -> Result<()> {
        send_cmd!(self, Write, characteristic.uuid, data.to_vec(), write_type)
    }

    async fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>> {
        send_cmd!(self, Read, characteristic.uuid)
    }

    async fn subscribe(&self, characteristic: &Characteristic) -> Result<()> {
        send_cmd!(self, Subscribe, characteristic.uuid)
    }

    async fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()> {
        send_cmd!(self, Unsubscribe, characteristic.uuid)
    }

    async fn notifications(&self) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>> {
        let receiver = self.shared.notifications_channel.subscribe();
        Ok(notifications_stream_from_broadcast_receiver(receiver))
    }
}

impl From<BluetoothCharacteristicProperties> for CharPropFlags {
    fn from(flags: BluetoothCharacteristicProperties) -> Self {
        let mut result = CharPropFlags::default();
        if flags.broadcast() {
            result.insert(CharPropFlags::BROADCAST);
        }
        if flags.read() {
            result.insert(CharPropFlags::READ);
        }
        if flags.write_without_response() {
            result.insert(CharPropFlags::WRITE_WITHOUT_RESPONSE);
        }
        if flags.write() {
            result.insert(CharPropFlags::WRITE);
        }
        if flags.notify() {
            result.insert(CharPropFlags::NOTIFY);
        }
        if flags.indicate() {
            result.insert(CharPropFlags::INDICATE);
        }
        if flags.authenticated_signed_writes() {
            result.insert(CharPropFlags::AUTHENTICATED_SIGNED_WRITES);
        }
        result
    }
}

impl From<String> for PeripheralId {
    fn from(id: String) -> Self {
        PeripheralId(id)
    }
}

impl Debug for Peripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let shared = &self.shared;

        f.debug_struct("Peripheral")
            .field("id", &shared.id)
            .field("name", &shared.name)
            .finish()
    }
}
