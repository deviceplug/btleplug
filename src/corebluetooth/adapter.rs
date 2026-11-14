use super::internal::{
    CoreBluetoothEvent, CoreBluetoothMessage, CoreBluetoothReply, CoreBluetoothReplyFuture,
    run_corebluetooth_thread,
};
use super::peripheral::{Peripheral, PeripheralId};
use crate::api::{
    BDAddr, Central, CentralEvent, CentralState, Peripheral as PeripheralTrait,
    RetrievePeripheralsOptions, ScanFilter,
};
use crate::common::adapter_manager::AdapterManager;
use crate::{Error, Result};
use async_trait::async_trait;
use futures::channel::mpsc::{self, Sender};
use futures::channel::oneshot;
use futures::sink::SinkExt;
use futures::stream::{Stream, StreamExt};
use log::*;
use objc2_core_bluetooth::CBManagerState;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::task;

/// Implementation of [api::Central](crate::api::Central).
#[derive(Clone, Debug)]
pub struct Adapter {
    manager: Arc<AdapterManager<Peripheral>>,
    sender: Sender<CoreBluetoothMessage>,
    keep_alive: KeepAlive,
}

/// Keeps an adapter's CoreBluetooth thread running. The adapter, every
/// peripheral it hands out and the event and notification streams from them
/// hold one; the thread stops when the last is dropped.
/// The adapter's own bookkeeping (its event task and peripheral map) doesn't
/// hold one, since it lives as long as the thread does.
pub(crate) type KeepAlive = Arc<oneshot::Sender<()>>;

/// Wraps a stream handed to the application so that it holds a `KeepAlive`,
/// and keeps delivering items after the adapter it came from is dropped.
pub(crate) fn keep_alive_stream<T: 'static>(
    stream: Pin<Box<dyn Stream<Item = T> + Send>>,
    keep_alive: Option<KeepAlive>,
) -> Pin<Box<dyn Stream<Item = T> + Send>> {
    Box::pin(KeepAliveStream {
        stream,
        _keep_alive: keep_alive,
    })
}

struct KeepAliveStream<T> {
    stream: Pin<Box<dyn Stream<Item = T> + Send>>,
    _keep_alive: Option<KeepAlive>,
}

impl<T> Stream for KeepAliveStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.stream.as_mut().poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

fn get_central_state(state: CBManagerState) -> CentralState {
    match state {
        CBManagerState::PoweredOn => CentralState::PoweredOn,
        CBManagerState::PoweredOff => CentralState::PoweredOff,
        _ => CentralState::Unknown,
    }
}

impl Adapter {
    pub(crate) async fn new() -> Result<Self> {
        let (sender, mut receiver) = mpsc::channel(256);
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let adapter_sender = run_corebluetooth_thread(sender, shutdown_receiver)?;
        // Since init currently blocked until the state update, we know the
        // receiver is dropped after that. We can pick it up here and make it
        // part of our event loop to update our peripherals.
        debug!("Waiting on adapter connect");
        if !matches!(
            receiver.next().await,
            Some(CoreBluetoothEvent::DidUpdateState { state: _ })
        ) {
            return Err(Error::Other(
                "Adapter failed to connect.".to_string().into(),
            ));
        }
        debug!("Adapter connected");
        let manager = Arc::new(AdapterManager::default());

        let manager_clone = manager.clone();
        let adapter_sender_clone = adapter_sender.clone();
        task::spawn(async move {
            let mut handles = HashMap::new();
            while let Some(msg) = receiver.next().await {
                match msg {
                    CoreBluetoothEvent::DeviceDiscovered {
                        uuid,
                        local_name,
                        advertisement_name,
                        event_receiver,
                    } => {
                        if manager_clone.peripheral(&uuid.into()).is_none() {
                            let peripheral = Peripheral::new(
                                uuid,
                                local_name,
                                advertisement_name,
                                Arc::downgrade(&manager_clone),
                                event_receiver,
                                adapter_sender_clone.clone(),
                            );
                            let peripheral = manager_clone.add_peripheral(peripheral);
                            handles.insert(peripheral.id(), peripheral);
                            manager_clone.emit(CentralEvent::DeviceDiscovered(uuid.into()));
                        }
                    }
                    CoreBluetoothEvent::RetrievedPeripherals {
                        peripherals,
                        future,
                    } => {
                        let mut result = Vec::with_capacity(peripherals.len());
                        for retrieved in peripherals {
                            let id = retrieved.uuid.into();
                            let peripheral = if let Some(peripheral) = handles.get(&id).cloned() {
                                peripheral.update_name(
                                    retrieved.local_name.clone(),
                                    retrieved.advertisement_name.clone(),
                                );
                                peripheral
                            } else if let Some(event_receiver) = retrieved.event_receiver {
                                let peripheral = Peripheral::new(
                                    retrieved.uuid,
                                    retrieved.local_name,
                                    retrieved.advertisement_name,
                                    Arc::downgrade(&manager_clone),
                                    event_receiver,
                                    adapter_sender_clone.clone(),
                                );
                                handles.insert(id.clone(), peripheral.clone());
                                peripheral
                            } else {
                                continue;
                            };

                            let peripheral = if manager_clone.peripheral(&id).is_none() {
                                let peripheral = manager_clone.add_peripheral(peripheral);
                                manager_clone.emit(CentralEvent::DeviceDiscovered(id));
                                peripheral
                            } else {
                                peripheral
                            };
                            result.push(peripheral);
                        }
                        future
                            .lock()
                            .unwrap()
                            .set_reply(CoreBluetoothReply::Peripherals(result));
                    }
                    CoreBluetoothEvent::DeviceUpdated {
                        uuid,
                        local_name,
                        advertisement_name,
                    } => {
                        let id = uuid.into();
                        if let Some(entry) = manager_clone.peripheral_mut(&id) {
                            entry.value().update_name(local_name, advertisement_name);
                            manager_clone.emit(CentralEvent::DeviceUpdated(id));
                        }
                    }
                    CoreBluetoothEvent::DeviceDisconnected { uuid } => {
                        handles.remove(&uuid.into());
                        manager_clone.emit(CentralEvent::DeviceDisconnected(uuid.into()));
                    }
                    CoreBluetoothEvent::PeripheralsCleared { future } => {
                        manager_clone.clear_peripherals();
                        handles.clear();
                        future.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
                    }
                    CoreBluetoothEvent::DidUpdateState { state } => {
                        let central_state = get_central_state(state);
                        manager_clone.emit(CentralEvent::StateUpdate(central_state));
                    }
                }
            }
        });

        Ok(Adapter {
            manager,
            sender: adapter_sender,
            keep_alive: Arc::new(shutdown_sender),
        })
    }
}

#[async_trait]
impl Central for Adapter {
    type Peripheral = Peripheral;

    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent> + Send>>> {
        Ok(keep_alive_stream(
            self.manager.event_stream(),
            Some(self.keep_alive.clone()),
        ))
    }

    async fn start_scan(&self, filter: ScanFilter) -> Result<()> {
        self.sender
            .to_owned()
            .send(CoreBluetoothMessage::StartScanning { filter })
            .await?;
        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        self.sender
            .to_owned()
            .send(CoreBluetoothMessage::StopScanning)
            .await?;
        Ok(())
    }

    async fn peripherals(&self) -> Result<Vec<Peripheral>> {
        Ok(self
            .manager
            .peripherals()
            .into_iter()
            .map(|p| p.with_keep_alive(&self.keep_alive))
            .collect())
    }

    async fn retrieve_peripherals(
        &self,
        options: RetrievePeripheralsOptions,
    ) -> Result<Vec<Peripheral>> {
        if options.identifiers.is_none() && options.services.is_none() {
            return Err(Error::NotSupported("retrieve_peripherals".to_string()));
        }
        if options.identifiers.as_ref().is_some_and(Vec::is_empty)
            && options.services.as_ref().is_none_or(Vec::is_empty)
        {
            return Ok(Vec::new());
        }
        let fut = CoreBluetoothReplyFuture::default();
        self.sender
            .to_owned()
            .send(CoreBluetoothMessage::RetrievePeripherals {
                options,
                future: fut.get_state_clone(),
            })
            .await?;
        match fut.await {
            CoreBluetoothReply::Peripherals(peripherals) => Ok(peripherals
                .into_iter()
                .map(|p| p.with_keep_alive(&self.keep_alive))
                .collect()),
            CoreBluetoothReply::Err(msg) => Err(Error::RuntimeError(msg)),
            CoreBluetoothReply::Ok => Ok(Vec::new()),
            _ => Err(Error::RuntimeError(
                "Unexpected CoreBluetooth retrieval reply".to_string(),
            )),
        }
    }

    async fn peripheral(&self, id: &PeripheralId) -> Result<Peripheral> {
        self.manager
            .peripheral(id)
            .map(|p| p.with_keep_alive(&self.keep_alive))
            .ok_or(Error::DeviceNotFound)
    }

    async fn add_peripheral(&self, _address: &PeripheralId) -> Result<Peripheral> {
        Err(Error::NotSupported(
            "Can't add a Peripheral from a PeripheralId".to_string(),
        ))
    }

    async fn clear_peripherals(&self) -> Result<()> {
        let fut = CoreBluetoothReplyFuture::default();
        self.sender
            .to_owned()
            .send(CoreBluetoothMessage::ClearPeripherals {
                future: fut.get_state_clone(),
            })
            .await
            .map_err(|e| Error::Other(Box::new(e)))?;
        match fut.await {
            CoreBluetoothReply::Ok => Ok(()),
            _ => Err(Error::RuntimeError(
                "Unexpected CoreBluetooth clear reply".to_string(),
            )),
        }
    }

    async fn adapter_info(&self) -> Result<String> {
        // TODO: Get information about the adapter.
        Ok("CoreBluetooth".to_string())
    }

    async fn adapter_address(&self) -> Result<Option<BDAddr>> {
        // CoreBluetooth exposes opaque UUID identities, not controller addresses.
        Ok(None)
    }

    async fn adapter_state(&self) -> Result<CentralState> {
        let fut = CoreBluetoothReplyFuture::default();
        self.sender
            .to_owned()
            .send(CoreBluetoothMessage::GetAdapterState {
                future: fut.get_state_clone(),
            })
            .await?;

        match fut.await {
            CoreBluetoothReply::AdapterState(state) => {
                let central_state = get_central_state(state);
                return Ok(central_state.clone());
            }
            _ => panic!("Shouldn't get anything but a AdapterState!"),
        }
    }
}
