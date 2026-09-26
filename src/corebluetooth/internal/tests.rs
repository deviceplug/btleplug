use super::*;
use futures::StreamExt;
use objc2::{DefinedClass, define_class};
use objc2_core_bluetooth::{
    CBAttributePermissions, CBMutableCharacteristic, CBMutableDescriptor, CBMutableService,
    CBPeripheralDelegate,
};
use objc2_foundation::{NSError, NSObjectProtocol, NSString, ns_string};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::task::Poll;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecordedSetNotify {
    characteristic_uuid: Uuid,
    enabled: bool,
}

struct TestPeripheralState {
    identifier: Retained<NSUUID>,
    set_notify_calls: Mutex<Vec<RecordedSetNotify>>,
}

define_class!(
    #[unsafe(super(CBPeripheral))]
    #[thread_kind = AnyThread]
    #[ivars = TestPeripheralState]
    struct TestPeripheral;

    unsafe impl NSObjectProtocol for TestPeripheral {}

    impl TestPeripheral {
        #[unsafe(method_id(identifier))]
        fn identifier(&self) -> Retained<NSUUID> {
            self.ivars().identifier.clone()
        }

        #[unsafe(method_id(name))]
        fn name(&self) -> Option<Retained<NSString>> {
            None
        }

        #[unsafe(method(maximumWriteValueLengthForType:))]
        fn maximum_write_value_length_for_type(
            &self,
            _write_type: CBCharacteristicWriteType,
        ) -> usize {
            0
        }

        #[unsafe(method(setNotifyValue:forCharacteristic:))]
        fn set_notify_value_for_characteristic(
            &self,
            enabled: bool,
            characteristic: &CBCharacteristic,
        ) {
            let raw_uuid = unsafe { characteristic.UUID() };
            self.ivars()
                .set_notify_calls
                .lock()
                .unwrap()
                .push(RecordedSetNotify {
                    characteristic_uuid: cbuuid_to_uuid(&raw_uuid),
                    enabled,
                });
        }
    }
);

impl TestPeripheral {
    fn new(identifier: Retained<NSUUID>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(TestPeripheralState {
            identifier,
            set_notify_calls: Mutex::new(Vec::new()),
        });
        unsafe { msg_send![super(this), init] }
    }

    fn set_notify_calls(&self) -> Vec<RecordedSetNotify> {
        self.ivars().set_notify_calls.lock().unwrap().clone()
    }
}

define_class!(
    #[unsafe(super(CBMutableCharacteristic))]
    #[thread_kind = AnyThread]
    #[ivars = AtomicBool]
    struct TestCharacteristic;

    unsafe impl NSObjectProtocol for TestCharacteristic {}

    impl TestCharacteristic {
        #[unsafe(method(isNotifying))]
        fn is_notifying(&self) -> bool {
            self.ivars().load(Ordering::SeqCst)
        }
    }
);

impl TestCharacteristic {
    fn new(uuid: &CBUUID, properties: CBCharacteristicProperties) -> Retained<Self> {
        let this = Self::alloc().set_ivars(AtomicBool::new(false));
        unsafe {
            msg_send![super(this),
                initWithType: uuid,
                properties: properties,
                value: None::<&NSData>,
                permissions: CBAttributePermissions::Readable
            ]
        }
    }
}

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(2);

struct FixtureCharacteristic {
    uuid: Uuid,
    characteristic: Retained<TestCharacteristic>,
}

struct NotificationFixture {
    peripheral_uuid: Uuid,
    service_uuid: Uuid,
    peripheral: Retained<TestPeripheral>,
    internal: PeripheralInternal,
    delegate: Retained<CentralDelegate>,
    delegate_receiver: Receiver<CentralDelegateEvent>,
    characteristics: Vec<FixtureCharacteristic>,
}

impl NotificationFixture {
    fn new(characteristic_uuids: &[Uuid]) -> Self {
        let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
        let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
        let peripheral_uuid_string = NSString::from_str(&peripheral_uuid.to_string());
        let peripheral_identifier =
            NSUUID::initWithUUIDString(NSUUID::alloc(), &peripheral_uuid_string)
                .expect("valid peripheral UUID");
        let peripheral = TestPeripheral::new(peripheral_identifier);
        // Permanently retain the test peripheral so the fixture can drop
        // at any point, including during a panicking assertion, without
        // ever running CBPeripheral's private destruction path.
        std::mem::forget(peripheral.clone());

        let characteristics: Vec<FixtureCharacteristic> = characteristic_uuids
            .iter()
            .map(|&uuid| {
                let characteristic_cbuuid = uuid_to_cbuuid(uuid);
                let characteristic = TestCharacteristic::new(
                    &characteristic_cbuuid,
                    CBCharacteristicProperties::Notify | CBCharacteristicProperties::Read,
                );
                FixtureCharacteristic {
                    uuid,
                    characteristic,
                }
            })
            .collect();
        let native_characteristics = characteristics
            .iter()
            .map(|fixture_characteristic| {
                Retained::into_super(Retained::into_super(
                    fixture_characteristic.characteristic.clone(),
                ))
            })
            .collect::<Vec<Retained<CBCharacteristic>>>();

        let service_cbuuid = uuid_to_cbuuid(service_uuid);
        let service = unsafe {
            CBMutableService::initWithType_primary(CBMutableService::alloc(), &service_cbuuid, true)
        };
        let attached_characteristics = NSArray::from_retained_slice(&native_characteristics);
        unsafe { service.setCharacteristics(Some(&attached_characteristics)) };

        let (event_sender, _) = mpsc::channel(1);
        let mut internal =
            PeripheralInternal::new(Retained::into_super(peripheral.clone()), event_sender);
        internal.services.insert(
            service_uuid,
            ServiceInternal {
                cbservice: Retained::into_super(service),
                characteristics: native_characteristics
                    .into_iter()
                    .zip(characteristic_uuids)
                    .map(|(characteristic, &uuid)| {
                        (uuid, CharacteristicInternal::new(characteristic))
                    })
                    .collect(),
                discovered: true,
            },
        );

        let (delegate_sender, delegate_receiver) = mpsc::channel(4);
        let delegate = CentralDelegate::new(delegate_sender);

        Self {
            peripheral_uuid,
            service_uuid,
            peripheral,
            internal,
            delegate,
            delegate_receiver,
            characteristics,
        }
    }

    fn characteristic(&self, index: usize) -> &TestCharacteristic {
        &self.characteristics[index].characteristic
    }

    fn set_notify_calls(&self) -> Vec<RecordedSetNotify> {
        self.peripheral.set_notify_calls()
    }
}

/// Run the real Objective-C notification-state delegate method, drain the
/// resulting delegate event from the channel, and dispatch it through the
/// production handler. Returns the callback's localized error text, if any.
async fn deliver_notification_state_callback(
    fixture: &mut NotificationFixture,
    characteristic_index: usize,
    error: Option<&NSError>,
    context: &str,
) -> Option<String> {
    let peripheral = &fixture.peripheral;
    let characteristic = &fixture.characteristics[characteristic_index].characteristic;
    unsafe {
        fixture
            .delegate
            .peripheral_didUpdateNotificationStateForCharacteristic_error(
                peripheral,
                characteristic,
                error,
            );
    }
    let event = tokio::time::timeout(CALLBACK_TIMEOUT, fixture.delegate_receiver.next())
        .await
        .unwrap_or_else(|_| panic!("{context}: notification state callback did not emit an event"))
        .unwrap_or_else(|| panic!("{context}: delegate event channel closed"));
    let CentralDelegateEvent::CharacteristicNotificationStateUpdated {
        peripheral_uuid,
        service_uuid,
        characteristic_uuid,
        error: event_error,
    } = event
    else {
        panic!("{context}: unexpected delegate event: {event:?}");
    };
    assert_eq!(peripheral_uuid, fixture.peripheral_uuid, "{context}");
    assert_eq!(service_uuid, fixture.service_uuid, "{context}");
    assert_eq!(
        characteristic_uuid, fixture.characteristics[characteristic_index].uuid,
        "{context}"
    );
    let expected_error = error.map(|error| error.localizedDescription().to_string());
    assert_eq!(event_error, expected_error, "{context}");
    fixture
        .internal
        .on_notification_state_updated(service_uuid, characteristic_uuid, event_error);
    expected_error
}

async fn assert_pending(future: &mut CoreBluetoothReplyFuture, context: &str) {
    assert!(
        matches!(futures::poll!(future), Poll::Pending),
        "{context}: future unexpectedly completed"
    );
}

fn notification_error(att_error: bool) -> Retained<NSError> {
    if att_error {
        // Error code 15 mirrors the ATT "Insufficient Encryption" refusal
        // from the issue report.
        NSError::new(15, ns_string!("ATT"))
    } else {
        NSError::new(1, ns_string!("BtlePlugCoreBluetoothTests"))
    }
}

#[test]
fn maximum_write_value_length_is_converted_to_att_mtu() {
    assert_eq!(maximum_write_value_length_to_att_mtu(20), Ok(23));
    assert_eq!(maximum_write_value_length_to_att_mtu(512), Ok(515));
    assert_eq!(
        maximum_write_value_length_to_att_mtu(u16::MAX as usize - 3),
        Ok(u16::MAX)
    );
}

#[test]
fn zero_maximum_write_value_length_uses_default_mtu() {
    assert_eq!(
        maximum_write_value_length_to_att_mtu(0),
        Ok(crate::api::DEFAULT_MTU_SIZE)
    );
}

#[test]
fn unrepresentable_maximum_write_value_length_is_rejected() {
    assert!(maximum_write_value_length_to_att_mtu(u16::MAX as usize).is_err());
    assert!(maximum_write_value_length_to_att_mtu(usize::MAX).is_err());
}

#[tokio::test]
async fn characteristic_discovery_error_preserves_pending_characteristic_read() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let characteristic_cbuuid = uuid_to_cbuuid(characteristic_uuid);
    let characteristic = unsafe {
        CBMutableCharacteristic::initWithType_properties_value_permissions(
            CBMutableCharacteristic::alloc(),
            &characteristic_cbuuid,
            CBCharacteristicProperties::Read,
            None,
            CBAttributePermissions::Readable,
        )
    };
    let characteristic: Retained<CBCharacteristic> = Retained::into_super(characteristic);
    let mut characteristic_internal = CharacteristicInternal::new(characteristic);
    characteristic_internal.discovered = true;
    let mut pending_read = CoreBluetoothReplyFuture::default();
    characteristic_internal
        .read_future_state
        .push_back(pending_read.get_state_clone());
    let service = unsafe {
        CBMutableService::initWithType_primary(
            CBMutableService::alloc(),
            &uuid_to_cbuuid(service_uuid),
            true,
        )
    };
    internal.services.insert(
        service_uuid,
        ServiceInternal {
            cbservice: Retained::into_super(service),
            characteristics: HashMap::from([(characteristic_uuid, characteristic_internal)]),
            discovered: true,
        },
    );

    // A late/duplicate error callback (#167) must not prune the
    // characteristic or its in-flight read.
    internal.set_characteristics(service_uuid, HashMap::new(), true);

    assert_pending(&mut pending_read, "pending read after characteristic error").await;
    let service = internal
        .services
        .get(&service_uuid)
        .expect("service preserved");
    assert!(
        service.characteristics.contains_key(&characteristic_uuid),
        "characteristic must survive an error callback"
    );

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn descriptor_discovery_error_preserves_pending_descriptor_read() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let characteristic_cbuuid = uuid_to_cbuuid(characteristic_uuid);
    let characteristic = unsafe {
        CBMutableCharacteristic::initWithType_properties_value_permissions(
            CBMutableCharacteristic::alloc(),
            &characteristic_cbuuid,
            CBCharacteristicProperties::Read,
            None,
            CBAttributePermissions::Readable,
        )
    };
    let characteristic: Retained<CBCharacteristic> = Retained::into_super(characteristic);
    let mut characteristic_internal = CharacteristicInternal::new(characteristic);
    characteristic_internal.discovered = true;
    let descriptor_uuid = Uuid::from_u128(0x00002902_0000_1000_8000_00805f9b34fb);
    let descriptor_cbuuid = uuid_to_cbuuid(descriptor_uuid);
    let descriptor_value = NSData::from_vec(vec![0u8]);
    let descriptor = unsafe {
        CBMutableDescriptor::initWithType_value(
            CBMutableDescriptor::alloc(),
            &descriptor_cbuuid,
            Some(&descriptor_value),
        )
    };
    let mut descriptor_internal = DescriptorInternal::new(Retained::into_super(descriptor));
    let mut pending_read = CoreBluetoothReplyFuture::default();
    descriptor_internal
        .read_future_state
        .push_back(pending_read.get_state_clone());
    characteristic_internal
        .descriptors
        .insert(descriptor_uuid, descriptor_internal);
    let service = unsafe {
        CBMutableService::initWithType_primary(
            CBMutableService::alloc(),
            &uuid_to_cbuuid(service_uuid),
            true,
        )
    };
    internal.services.insert(
        service_uuid,
        ServiceInternal {
            cbservice: Retained::into_super(service),
            characteristics: HashMap::from([(characteristic_uuid, characteristic_internal)]),
            discovered: true,
        },
    );

    // A late/duplicate error callback (#167) must not prune the
    // descriptor or its in-flight read.
    internal.set_characteristic_descriptors(
        service_uuid,
        characteristic_uuid,
        HashMap::new(),
        true,
    );

    assert_pending(&mut pending_read, "pending read after descriptor error").await;
    let service = internal.services.get(&service_uuid).expect("service");
    let characteristic = service
        .characteristics
        .get(&characteristic_uuid)
        .expect("characteristic");
    assert!(
        characteristic.descriptors.contains_key(&descriptor_uuid),
        "descriptor must survive an error callback"
    );

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn descriptor_discovery_error_completes_service_discovery_without_descriptors() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let peripheral_uuid_string = NSString::from_str(&peripheral_uuid.to_string());
    let peripheral_identifier =
        NSUUID::initWithUUIDString(NSUUID::alloc(), &peripheral_uuid_string)
            .expect("valid peripheral UUID");
    let peripheral = TestPeripheral::new(peripheral_identifier);
    let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let service_cbuuid = uuid_to_cbuuid(service_uuid);
    let characteristic_cbuuid = uuid_to_cbuuid(characteristic_uuid);
    let characteristic = unsafe {
        CBMutableCharacteristic::initWithType_properties_value_permissions(
            CBMutableCharacteristic::alloc(),
            &characteristic_cbuuid,
            CBCharacteristicProperties::Read,
            None,
            CBAttributePermissions::Readable,
        )
    };
    let service = unsafe {
        CBMutableService::initWithType_primary(CBMutableService::alloc(), &service_cbuuid, true)
    };
    let characteristic: Retained<CBCharacteristic> = Retained::into_super(characteristic);
    let characteristics = NSArray::from_retained_slice(std::slice::from_ref(&characteristic));
    unsafe { service.setCharacteristics(Some(&characteristics)) };
    let service: Retained<CBService> = Retained::into_super(service);

    let (event_sender, _) = mpsc::channel(1);
    let mut internal =
        PeripheralInternal::new(Retained::into_super(peripheral.clone()), event_sender);
    internal.services.insert(
        service_uuid,
        ServiceInternal {
            cbservice: service,
            characteristics: HashMap::from([(
                characteristic_uuid,
                CharacteristicInternal::new(characteristic.clone()),
            )]),
            discovered: false,
        },
    );
    let discovery = CoreBluetoothReplyFuture::default();
    internal
        .services_discovered_future_state
        .push_back(discovery.get_state_clone());

    let (delegate_sender, mut delegate_receiver) = mpsc::channel(1);
    let delegate = CentralDelegate::new(delegate_sender);
    let error = NSError::new(1, ns_string!("BtlePlugCoreBluetoothTests"));
    unsafe {
        delegate.peripheral_didDiscoverDescriptorsForCharacteristic_error(
            &peripheral,
            &characteristic,
            Some(&error),
        );
    }

    let event = tokio::time::timeout(Duration::from_secs(1), delegate_receiver.next())
        .await
        .expect("descriptor error callback did not emit an event")
        .expect("delegate event channel closed");
    let CentralDelegateEvent::DiscoveredCharacteristicDescriptors {
        peripheral_uuid: event_peripheral_uuid,
        service_uuid: event_service_uuid,
        characteristic_uuid: event_characteristic_uuid,
        descriptors,
        error: event_error,
    } = event
    else {
        panic!("unexpected delegate event: {event:?}");
    };
    assert_eq!(event_peripheral_uuid, peripheral_uuid);
    assert_eq!(event_service_uuid, service_uuid);
    assert_eq!(event_characteristic_uuid, characteristic_uuid);
    assert!(descriptors.is_empty());
    assert!(event_error);

    internal.set_characteristic_descriptors(
        event_service_uuid,
        event_characteristic_uuid,
        descriptors,
        event_error,
    );
    let reply = tokio::time::timeout(Duration::from_secs(1), discovery)
        .await
        .expect("service discovery remained pending after descriptor error");
    let CoreBluetoothReply::ServicesDiscovered(services, mtu) = reply else {
        panic!("unexpected discovery reply: {reply:?}");
    };
    assert_eq!(mtu, crate::api::DEFAULT_MTU_SIZE);
    let characteristic = services
        .iter()
        .find(|service| service.uuid == service_uuid)
        .and_then(|service| {
            service
                .characteristics
                .iter()
                .find(|characteristic| characteristic.uuid == characteristic_uuid)
        })
        .expect("discovered characteristic");
    assert!(characteristic.descriptors.is_empty());

    // CBPeripheral has no public initializer suitable for tests, so this
    // subclass must not run CoreBluetooth's private destruction path.
    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn characteristic_discovery_error_completes_service_with_no_characteristics() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let service_cbuuid = uuid_to_cbuuid(service_uuid);
    let service = unsafe {
        CBMutableService::initWithType_primary(CBMutableService::alloc(), &service_cbuuid, true)
    };
    let service: Retained<CBService> = Retained::into_super(service);
    internal.services.insert(
        service_uuid,
        ServiceInternal {
            cbservice: service.clone(),
            characteristics: HashMap::new(),
            discovered: false,
        },
    );
    let discovery = CoreBluetoothReplyFuture::default();
    internal
        .services_discovered_future_state
        .push_back(discovery.get_state_clone());

    let (delegate_sender, mut delegate_receiver) = mpsc::channel(1);
    let delegate = CentralDelegate::new(delegate_sender);
    let error = NSError::new(1, ns_string!("BtlePlugCoreBluetoothTests"));
    unsafe {
        delegate.peripheral_didDiscoverCharacteristicsForService_error(
            &peripheral,
            &service,
            Some(&error),
        );
    }

    let event = tokio::time::timeout(Duration::from_secs(1), delegate_receiver.next())
        .await
        .expect("characteristic discovery error callback did not emit an event")
        .expect("delegate event channel closed");
    let CentralDelegateEvent::DiscoveredCharacteristics {
        peripheral_uuid: event_peripheral_uuid,
        service_uuid: event_service_uuid,
        characteristics,
        error: event_error,
    } = event
    else {
        panic!("unexpected delegate event: {event:?}");
    };
    assert_eq!(event_peripheral_uuid, peripheral_uuid);
    assert_eq!(event_service_uuid, service_uuid);
    assert!(characteristics.is_empty());
    assert!(event_error);

    internal.set_characteristics(event_service_uuid, characteristics, event_error);
    let reply = tokio::time::timeout(Duration::from_secs(1), discovery)
        .await
        .expect("service discovery remained pending after characteristic discovery error");
    let CoreBluetoothReply::ServicesDiscovered(services, _mtu) = reply else {
        panic!("unexpected discovery reply: {reply:?}");
    };
    let discovered_service = services
        .iter()
        .find(|service| service.uuid == service_uuid)
        .expect("discovered service");
    assert!(discovered_service.characteristics.is_empty());

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn service_discovery_error_emits_error_event() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, internal) = new_test_internal(peripheral_uuid);

    let (delegate_sender, mut delegate_receiver) = mpsc::channel(1);
    let delegate = CentralDelegate::new(delegate_sender);
    let error = NSError::new(1, ns_string!("BtlePlugCoreBluetoothTests"));
    unsafe {
        delegate.peripheral_didDiscoverServices(&peripheral, Some(&error));
    }

    let event = tokio::time::timeout(Duration::from_secs(1), delegate_receiver.next())
        .await
        .expect("service discovery error callback did not emit an event")
        .expect("delegate event channel closed");
    let CentralDelegateEvent::DiscoveredServices {
        peripheral_uuid: event_peripheral_uuid,
        services,
        error: event_error,
    } = event
    else {
        panic!("unexpected delegate event: {event:?}");
    };
    assert_eq!(event_peripheral_uuid, peripheral_uuid);
    assert!(services.is_empty());
    assert!(event_error.is_some());

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

fn new_test_internal(peripheral_uuid: Uuid) -> (Retained<TestPeripheral>, PeripheralInternal) {
    let peripheral_uuid_string = NSString::from_str(&peripheral_uuid.to_string());
    let peripheral_identifier =
        NSUUID::initWithUUIDString(NSUUID::alloc(), &peripheral_uuid_string)
            .expect("valid peripheral UUID");
    let peripheral = TestPeripheral::new(peripheral_identifier);
    let (event_sender, _) = mpsc::channel(1);
    let internal = PeripheralInternal::new(Retained::into_super(peripheral.clone()), event_sender);
    (peripheral, internal)
}

#[test]
fn set_characteristics_for_unknown_service_does_not_panic() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    assert!(internal.services.is_empty());

    internal.set_characteristics(
        Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb),
        HashMap::new(),
        false,
    );
    assert!(internal.services.is_empty());

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[test]
fn check_discovered_with_no_waiting_future_does_not_panic() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let service_cbuuid = uuid_to_cbuuid(service_uuid);
    let service = unsafe {
        CBMutableService::initWithType_primary(CBMutableService::alloc(), &service_cbuuid, true)
    };
    internal.services.insert(
        service_uuid,
        ServiceInternal {
            cbservice: Retained::into_super(service),
            characteristics: HashMap::new(),
            discovered: true,
        },
    );
    assert!(internal.services_discovered_future_state.is_empty());

    internal.check_discovered();

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn descriptors_for_unknown_service_leave_discovery_pending() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let service_cbuuid = uuid_to_cbuuid(service_uuid);
    let characteristic_cbuuid = uuid_to_cbuuid(characteristic_uuid);
    let characteristic = unsafe {
        CBMutableCharacteristic::initWithType_properties_value_permissions(
            CBMutableCharacteristic::alloc(),
            &characteristic_cbuuid,
            CBCharacteristicProperties::Read,
            None,
            CBAttributePermissions::Readable,
        )
    };
    let service = unsafe {
        CBMutableService::initWithType_primary(CBMutableService::alloc(), &service_cbuuid, true)
    };
    let characteristic: Retained<CBCharacteristic> = Retained::into_super(characteristic);
    let characteristics = NSArray::from_retained_slice(std::slice::from_ref(&characteristic));
    unsafe { service.setCharacteristics(Some(&characteristics)) };
    let service: Retained<CBService> = Retained::into_super(service);
    internal.services.insert(
        service_uuid,
        ServiceInternal {
            cbservice: service,
            characteristics: HashMap::from([(
                characteristic_uuid,
                CharacteristicInternal::new(characteristic),
            )]),
            discovered: false,
        },
    );
    let mut discovery = CoreBluetoothReplyFuture::default();
    internal
        .services_discovered_future_state
        .push_back(discovery.get_state_clone());

    // A descriptor callback for a service this peripheral has never seen
    // (e.g. an included service dropped by set_characteristics) must not
    // fail the still-pending service discovery.
    let handled = internal.set_characteristic_descriptors(
        Uuid::from_u128(0x0000ffff_0000_1000_8000_00805f9b34fb),
        characteristic_uuid,
        HashMap::new(),
        false,
    );
    assert!(handled);

    assert_pending(
        &mut discovery,
        "discovery after unknown-service descriptors",
    )
    .await;

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn notification_callback_error_completes_requested_operation() {
    for enabled in [true, false] {
        for att_error in [false, true] {
            let context = format!("enabled={enabled} att_error={att_error}");
            let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
            let mut fixture = NotificationFixture::new(&[characteristic_uuid]);
            let future = CoreBluetoothReplyFuture::default();
            fixture.internal.queue_notification_request(
                fixture.service_uuid,
                characteristic_uuid,
                enabled,
                future.get_state_clone(),
            );
            assert_eq!(
                fixture.set_notify_calls(),
                vec![RecordedSetNotify {
                    characteristic_uuid,
                    enabled,
                }],
                "{context}"
            );
            let error = notification_error(att_error);
            let expected_error =
                deliver_notification_state_callback(&mut fixture, 0, Some(&error), &context)
                    .await
                    .expect("{context}: error callback carried no error");
            let reply = tokio::time::timeout(CALLBACK_TIMEOUT, future)
                .await
                .unwrap_or_else(|_| {
                    panic!("{context}: notification error did not complete the request")
                });
            match reply {
                CoreBluetoothReply::Err(actual) => {
                    assert_eq!(actual, expected_error, "{context}")
                }
                reply => panic!("{context}: unexpected reply: {reply:?}"),
            }
        }
    }
}

#[tokio::test]
async fn notification_callback_success_completes_requested_operation() {
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let mut fixture = NotificationFixture::new(&[characteristic_uuid]);

    let enable = CoreBluetoothReplyFuture::default();
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_uuid,
        true,
        enable.get_state_clone(),
    );
    deliver_notification_state_callback(&mut fixture, 0, None, "enable").await;
    assert!(
        matches!(
            tokio::time::timeout(CALLBACK_TIMEOUT, enable)
                .await
                .expect("enable did not complete"),
            CoreBluetoothReply::Ok
        ),
        "enable reply was not Ok"
    );

    let disable = CoreBluetoothReplyFuture::default();
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_uuid,
        false,
        disable.get_state_clone(),
    );
    deliver_notification_state_callback(&mut fixture, 0, None, "disable").await;
    assert!(
        matches!(
            tokio::time::timeout(CALLBACK_TIMEOUT, disable)
                .await
                .expect("disable did not complete"),
            CoreBluetoothReply::Ok
        ),
        "disable reply was not Ok"
    );

    // Repeated same-direction requests stay independent native calls and
    // futures; no coalescing.
    let first_repeat = CoreBluetoothReplyFuture::default();
    let mut second_repeat = CoreBluetoothReplyFuture::default();
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_uuid,
        true,
        first_repeat.get_state_clone(),
    );
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_uuid,
        true,
        second_repeat.get_state_clone(),
    );
    assert_eq!(fixture.set_notify_calls().len(), 3);
    deliver_notification_state_callback(&mut fixture, 0, None, "first repeat").await;
    assert!(
        matches!(
            tokio::time::timeout(CALLBACK_TIMEOUT, first_repeat)
                .await
                .expect("first repeat did not complete"),
            CoreBluetoothReply::Ok
        ),
        "first repeat reply was not Ok"
    );
    assert_pending(&mut second_repeat, "second repeat").await;
    deliver_notification_state_callback(&mut fixture, 0, None, "second repeat").await;
    assert!(
        matches!(
            tokio::time::timeout(CALLBACK_TIMEOUT, second_repeat)
                .await
                .expect("second repeat did not complete"),
            CoreBluetoothReply::Ok
        ),
        "second repeat reply was not Ok"
    );
    assert_eq!(
        fixture.set_notify_calls(),
        vec![
            RecordedSetNotify {
                characteristic_uuid,
                enabled: true
            },
            RecordedSetNotify {
                characteristic_uuid,
                enabled: false
            },
            RecordedSetNotify {
                characteristic_uuid,
                enabled: true
            },
            RecordedSetNotify {
                characteristic_uuid,
                enabled: true
            },
        ]
    );
}

#[tokio::test]
async fn notification_requests_are_serialized_per_characteristic() {
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let mut fixture = NotificationFixture::new(&[characteristic_uuid]);

    let first = CoreBluetoothReplyFuture::default();
    let mut second = CoreBluetoothReplyFuture::default();
    let mut third = CoreBluetoothReplyFuture::default();
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_uuid,
        true,
        first.get_state_clone(),
    );
    assert_eq!(
        fixture.set_notify_calls(),
        vec![RecordedSetNotify {
            characteristic_uuid,
            enabled: true
        }]
    );
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_uuid,
        false,
        second.get_state_clone(),
    );
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_uuid,
        true,
        third.get_state_clone(),
    );
    assert_eq!(
        fixture.set_notify_calls().len(),
        1,
        "only the queue head may be submitted"
    );
    assert_pending(&mut second, "second").await;
    assert_pending(&mut third, "third").await;

    deliver_notification_state_callback(&mut fixture, 0, None, "first").await;
    assert!(
        matches!(
            tokio::time::timeout(CALLBACK_TIMEOUT, first)
                .await
                .expect("first did not complete"),
            CoreBluetoothReply::Ok
        ),
        "first reply was not Ok"
    );
    assert_eq!(
        fixture.set_notify_calls(),
        vec![
            RecordedSetNotify {
                characteristic_uuid,
                enabled: true
            },
            RecordedSetNotify {
                characteristic_uuid,
                enabled: false
            },
        ]
    );
    assert_pending(&mut third, "third").await;

    let error = notification_error(true);
    let expected_error =
        deliver_notification_state_callback(&mut fixture, 0, Some(&error), "second")
            .await
            .expect("second callback carried no error");
    let second_reply = tokio::time::timeout(CALLBACK_TIMEOUT, second)
        .await
        .expect("second did not complete");
    assert!(
        matches!(second_reply, CoreBluetoothReply::Err(ref actual) if *actual == expected_error),
        "second reply was not the callback error: {second_reply:?}"
    );
    assert_eq!(
        fixture.set_notify_calls().len(),
        3,
        "completing the head must submit exactly the next request"
    );
    assert_pending(&mut third, "third after second callback").await;

    deliver_notification_state_callback(&mut fixture, 0, None, "third").await;
    assert!(
        matches!(
            tokio::time::timeout(CALLBACK_TIMEOUT, third)
                .await
                .expect("third did not complete"),
            CoreBluetoothReply::Ok
        ),
        "third reply was not Ok"
    );
    assert_eq!(fixture.set_notify_calls().len(), 3);
}

#[tokio::test]
async fn notification_requests_on_distinct_characteristics_are_independent() {
    let characteristic_a = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let characteristic_b = Uuid::from_u128(0x00002a37_0000_1000_8000_00805f9b34fb);
    let mut fixture = NotificationFixture::new(&[characteristic_a, characteristic_b]);

    let future_a = CoreBluetoothReplyFuture::default();
    let mut future_b = CoreBluetoothReplyFuture::default();
    let mut future_b_second = CoreBluetoothReplyFuture::default();
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_a,
        true,
        future_a.get_state_clone(),
    );
    assert_eq!(
        fixture.set_notify_calls(),
        vec![RecordedSetNotify {
            characteristic_uuid: characteristic_a,
            enabled: true
        }]
    );
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_b,
        true,
        future_b.get_state_clone(),
    );
    assert_eq!(
        fixture.set_notify_calls(),
        vec![
            RecordedSetNotify {
                characteristic_uuid: characteristic_a,
                enabled: true
            },
            RecordedSetNotify {
                characteristic_uuid: characteristic_b,
                enabled: true
            },
        ]
    );
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_b,
        false,
        future_b_second.get_state_clone(),
    );
    assert_eq!(
        fixture.set_notify_calls().len(),
        2,
        "each characteristic must own its queue"
    );

    let error = notification_error(true);
    let expected_error = deliver_notification_state_callback(&mut fixture, 0, Some(&error), "a")
        .await
        .expect("a callback carried no error");
    let reply_a = tokio::time::timeout(CALLBACK_TIMEOUT, future_a)
        .await
        .expect("a did not complete");
    assert!(
        matches!(reply_a, CoreBluetoothReply::Err(ref actual) if *actual == expected_error),
        "a reply was not the callback error: {reply_a:?}"
    );
    assert_pending(&mut future_b, "b").await;
    assert_pending(&mut future_b_second, "b second").await;
    assert_eq!(
        fixture.set_notify_calls().len(),
        2,
        "a callback for one characteristic must not advance another"
    );

    deliver_notification_state_callback(&mut fixture, 1, None, "b").await;
    assert!(
        matches!(
            tokio::time::timeout(CALLBACK_TIMEOUT, future_b)
                .await
                .expect("b did not complete"),
            CoreBluetoothReply::Ok
        ),
        "b reply was not Ok"
    );
    assert_eq!(fixture.set_notify_calls().len(), 3);
    assert_pending(&mut future_b_second, "b second after first b callback").await;
    deliver_notification_state_callback(&mut fixture, 1, None, "b second").await;
    assert!(
        matches!(
            tokio::time::timeout(CALLBACK_TIMEOUT, future_b_second)
                .await
                .expect("b second did not complete"),
            CoreBluetoothReply::Ok
        ),
        "b second reply was not Ok"
    );
}

#[tokio::test]
async fn cancelled_notification_waiter_does_not_steal_next_reply() {
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let mut fixture = NotificationFixture::new(&[characteristic_uuid]);

    let dropped_waiter = CoreBluetoothReplyFuture::default();
    let dropped_state = dropped_waiter.get_state_clone();
    drop(dropped_waiter);
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_uuid,
        true,
        dropped_state,
    );

    let mut survivor = CoreBluetoothReplyFuture::default();
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_uuid,
        false,
        survivor.get_state_clone(),
    );
    assert_eq!(
        fixture.set_notify_calls(),
        vec![RecordedSetNotify {
            characteristic_uuid,
            enabled: true
        }],
        "the second request must not be submitted while the first is in flight"
    );

    deliver_notification_state_callback(&mut fixture, 0, None, "dropped waiter").await;
    assert_eq!(
        fixture.set_notify_calls(),
        vec![
            RecordedSetNotify {
                characteristic_uuid,
                enabled: true
            },
            RecordedSetNotify {
                characteristic_uuid,
                enabled: false
            },
        ],
        "the dropped waiter's callback must submit the next request"
    );
    assert_pending(&mut survivor, "survivor").await;

    deliver_notification_state_callback(&mut fixture, 0, None, "survivor").await;
    assert!(
        matches!(
            tokio::time::timeout(CALLBACK_TIMEOUT, survivor)
                .await
                .expect("survivor did not complete"),
            CoreBluetoothReply::Ok
        ),
        "survivor reply was not Ok"
    );
}

#[tokio::test]
async fn disconnect_drains_active_and_queued_notification_requests() {
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let mut fixture = NotificationFixture::new(&[characteristic_uuid]);

    let first = CoreBluetoothReplyFuture::default();
    let second = CoreBluetoothReplyFuture::default();
    let third = CoreBluetoothReplyFuture::default();
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_uuid,
        true,
        first.get_state_clone(),
    );
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_uuid,
        false,
        second.get_state_clone(),
    );
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_uuid,
        true,
        third.get_state_clone(),
    );
    assert_eq!(fixture.set_notify_calls().len(), 1);

    fixture
        .internal
        .drain_pending_operations("Device disconnected");

    for (name, future) in [("first", first), ("second", second), ("third", third)] {
        let reply = tokio::time::timeout(CALLBACK_TIMEOUT, future)
            .await
            .unwrap_or_else(|_| panic!("{name} did not drain"));
        assert!(
            matches!(reply, CoreBluetoothReply::Err(ref message) if message == "Device disconnected"),
            "{name}: unexpected drain reply: {reply:?}"
        );
    }
    assert_eq!(
        fixture.set_notify_calls().len(),
        1,
        "draining must not submit more requests"
    );

    deliver_notification_state_callback(&mut fixture, 0, None, "late callback after drain").await;
    assert_eq!(
        fixture.set_notify_calls().len(),
        1,
        "a callback after draining must submit nothing"
    );
}

#[tokio::test]
async fn unexpected_notification_callback_is_ignored() {
    let characteristic_a = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let mut fixture = NotificationFixture::new(&[characteristic_a]);
    let error = notification_error(true);
    deliver_notification_state_callback(&mut fixture, 0, Some(&error), "callback with empty queue")
        .await;
    assert!(fixture.set_notify_calls().is_empty());

    let characteristic_b = Uuid::from_u128(0x00002a37_0000_1000_8000_00805f9b34fb);
    let mut fixture = NotificationFixture::new(&[characteristic_a, characteristic_b]);
    let mut future_a = CoreBluetoothReplyFuture::default();
    fixture.internal.queue_notification_request(
        fixture.service_uuid,
        characteristic_a,
        true,
        future_a.get_state_clone(),
    );
    deliver_notification_state_callback(
        &mut fixture,
        1,
        None,
        "callback for characteristic without a pending request",
    )
    .await;
    assert_eq!(
        fixture.set_notify_calls().len(),
        1,
        "unexpected callback must not consume another characteristic's request"
    );
    assert_pending(&mut future_a, "a after unrelated callback").await;
    deliver_notification_state_callback(&mut fixture, 0, None, "a").await;
    assert!(
        matches!(
            tokio::time::timeout(CALLBACK_TIMEOUT, future_a)
                .await
                .expect("a did not complete"),
            CoreBluetoothReply::Ok
        ),
        "a reply was not Ok"
    );
}

#[tokio::test]
async fn discover_services_error_completes_discovery_with_error() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let discovery = CoreBluetoothReplyFuture::default();
    internal
        .services_discovered_future_state
        .push_back(discovery.get_state_clone());

    internal.set_discovered_services(HashMap::new(), Some("boom".to_string()));

    let reply = tokio::time::timeout(Duration::from_secs(1), discovery)
        .await
        .expect("discovery remained pending after a service discovery error");
    assert!(
        matches!(reply, CoreBluetoothReply::Err(error) if error == "boom"),
        "expected an error reply carrying the delegate's error"
    );
    assert!(internal.services.is_empty());

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[test]
fn discover_services_error_with_no_waiting_future_does_not_panic() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    assert!(internal.services_discovered_future_state.is_empty());

    internal.set_discovered_services(HashMap::new(), Some("boom".to_string()));

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn discover_services_with_zero_services_completes_immediately() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let discovery = CoreBluetoothReplyFuture::default();
    internal
        .services_discovered_future_state
        .push_back(discovery.get_state_clone());

    internal.set_discovered_services(HashMap::new(), None);

    let reply = tokio::time::timeout(Duration::from_secs(1), discovery)
        .await
        .expect("discovery of an empty service set never completed");
    let CoreBluetoothReply::ServicesDiscovered(services, _mtu) = reply else {
        panic!("unexpected discovery reply: {reply:?}");
    };
    assert!(services.is_empty());

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn discover_services_with_pending_services_waits_for_characteristics() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let service_cbuuid = uuid_to_cbuuid(service_uuid);
    let service = unsafe {
        CBMutableService::initWithType_primary(CBMutableService::alloc(), &service_cbuuid, true)
    };
    let service: Retained<CBService> = Retained::into_super(service);
    let mut service_map = HashMap::new();
    service_map.insert(service_uuid, service);

    let mut discovery = CoreBluetoothReplyFuture::default();
    internal
        .services_discovered_future_state
        .push_back(discovery.get_state_clone());

    internal.set_discovered_services(service_map, None);

    assert_pending(&mut discovery, "discovery with an undiscovered service").await;

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn repeated_discovery_after_completion_does_not_hang() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let service_cbuuid = uuid_to_cbuuid(service_uuid);
    let service = unsafe {
        CBMutableService::initWithType_primary(CBMutableService::alloc(), &service_cbuuid, true)
    };
    let service: Retained<CBService> = Retained::into_super(service);
    let mut service_map = HashMap::new();
    service_map.insert(service_uuid, service);

    let first = CoreBluetoothReplyFuture::default();
    internal
        .services_discovered_future_state
        .push_back(first.get_state_clone());
    internal.set_discovered_services(service_map.clone(), None);
    internal.set_characteristics(service_uuid, HashMap::new(), false);
    tokio::time::timeout(Duration::from_secs(1), first)
        .await
        .expect("first discovery did not complete");
    assert!(internal.services_discovered_future_state.is_empty());

    let second = CoreBluetoothReplyFuture::default();
    internal
        .services_discovered_future_state
        .push_back(second.get_state_clone());
    internal.set_discovered_services(service_map, None);
    internal.set_characteristics(service_uuid, HashMap::new(), false);
    let reply = tokio::time::timeout(Duration::from_secs(1), second)
        .await
        .expect("second discovery hung");
    assert!(matches!(
        reply,
        CoreBluetoothReply::ServicesDiscovered(_, _)
    ));

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn concurrent_discovery_waiters_both_complete() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let service_cbuuid = uuid_to_cbuuid(service_uuid);
    let service = unsafe {
        CBMutableService::initWithType_primary(CBMutableService::alloc(), &service_cbuuid, true)
    };
    let service: Retained<CBService> = Retained::into_super(service);
    let mut service_map = HashMap::new();
    service_map.insert(service_uuid, service);

    let first = CoreBluetoothReplyFuture::default();
    let second = CoreBluetoothReplyFuture::default();
    internal
        .services_discovered_future_state
        .push_back(first.get_state_clone());
    internal
        .services_discovered_future_state
        .push_back(second.get_state_clone());

    internal.set_discovered_services(service_map, None);
    internal.set_characteristics(service_uuid, HashMap::new(), false);

    for (name, future) in [("first", first), ("second", second)] {
        let reply = tokio::time::timeout(Duration::from_secs(1), future)
            .await
            .unwrap_or_else(|_| panic!("{name} waiter never completed"));
        assert!(
            matches!(reply, CoreBluetoothReply::ServicesDiscovered(_, _)),
            "{name}: unexpected reply: {reply:?}"
        );
    }
    assert!(internal.services_discovered_future_state.is_empty());

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn concurrent_connect_waiters_both_complete() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);

    let first = CoreBluetoothReplyFuture::default();
    let second = CoreBluetoothReplyFuture::default();
    internal
        .connected_future_state
        .push_back(first.get_state_clone());
    internal
        .connected_future_state
        .push_back(second.get_state_clone());

    internal.complete_connect(CoreBluetoothReply::Connected);

    for (name, future) in [("first", first), ("second", second)] {
        let reply = tokio::time::timeout(Duration::from_secs(1), future)
            .await
            .unwrap_or_else(|_| panic!("{name} waiter never completed"));
        assert!(
            matches!(reply, CoreBluetoothReply::Connected),
            "{name}: unexpected reply: {reply:?}"
        );
    }
    assert!(internal.connected_future_state.is_empty());

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn drain_pending_operations_errors_all_queued_waiters() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);

    let connect_a = CoreBluetoothReplyFuture::default();
    let connect_b = CoreBluetoothReplyFuture::default();
    let discover_a = CoreBluetoothReplyFuture::default();
    let discover_b = CoreBluetoothReplyFuture::default();
    internal
        .connected_future_state
        .push_back(connect_a.get_state_clone());
    internal
        .connected_future_state
        .push_back(connect_b.get_state_clone());
    internal
        .services_discovered_future_state
        .push_back(discover_a.get_state_clone());
    internal
        .services_discovered_future_state
        .push_back(discover_b.get_state_clone());

    internal.drain_pending_operations("Device disconnected");

    for (name, future) in [
        ("connect_a", connect_a),
        ("connect_b", connect_b),
        ("discover_a", discover_a),
        ("discover_b", discover_b),
    ] {
        let reply = tokio::time::timeout(Duration::from_secs(1), future)
            .await
            .unwrap_or_else(|_| panic!("{name} did not drain"));
        assert!(
            matches!(reply, CoreBluetoothReply::Err(ref message) if message == "Device disconnected"),
            "{name}: unexpected drain reply: {reply:?}"
        );
    }
    assert!(internal.connected_future_state.is_empty());
    assert!(internal.services_discovered_future_state.is_empty());

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn rediscovery_preserves_pending_characteristic_read() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let service_cbuuid = uuid_to_cbuuid(service_uuid);
    let characteristic_cbuuid = uuid_to_cbuuid(characteristic_uuid);
    let characteristic = unsafe {
        CBMutableCharacteristic::initWithType_properties_value_permissions(
            CBMutableCharacteristic::alloc(),
            &characteristic_cbuuid,
            CBCharacteristicProperties::Read,
            None,
            CBAttributePermissions::Readable,
        )
    };
    let characteristic: Retained<CBCharacteristic> = Retained::into_super(characteristic);
    let service = unsafe {
        CBMutableService::initWithType_primary(CBMutableService::alloc(), &service_cbuuid, true)
    };
    let service: Retained<CBService> = Retained::into_super(service);
    internal.services.insert(
        service_uuid,
        ServiceInternal {
            cbservice: service,
            characteristics: HashMap::from([(
                characteristic_uuid,
                CharacteristicInternal::new(characteristic),
            )]),
            discovered: true,
        },
    );

    let mut read_future = CoreBluetoothReplyFuture::default();
    internal
        .services
        .get_mut(&service_uuid)
        .unwrap()
        .characteristics
        .get_mut(&characteristic_uuid)
        .unwrap()
        .read_future_state
        .push_back(read_future.get_state_clone());

    // Re-discovery hands back a new CBService object for the same UUID.
    let rediscovered_service = unsafe {
        CBMutableService::initWithType_primary(CBMutableService::alloc(), &service_cbuuid, true)
    };
    let rediscovered_service: Retained<CBService> = Retained::into_super(rediscovered_service);
    internal.set_discovered_services(HashMap::from([(service_uuid, rediscovered_service)]), None);

    assert_pending(&mut read_future, "read future after rediscovery").await;
    let preserved_service = internal
        .services
        .get(&service_uuid)
        .expect("service preserved");
    assert!(
        !preserved_service.discovered,
        "service must await re-discovery before completing again"
    );
    let preserved_characteristic = preserved_service
        .characteristics
        .get(&characteristic_uuid)
        .expect("characteristic preserved across rediscovery");
    assert_eq!(preserved_characteristic.read_future_state.len(), 1);

    // Simulate the read value arriving after rediscovery.
    let state = internal
        .services
        .get_mut(&service_uuid)
        .unwrap()
        .characteristics
        .get_mut(&characteristic_uuid)
        .unwrap()
        .read_future_state
        .pop_front()
        .expect("preserved read future");
    state
        .lock()
        .unwrap()
        .set_reply(CoreBluetoothReply::ReadResult(vec![1, 2, 3]));
    let reply = tokio::time::timeout(Duration::from_secs(1), read_future)
        .await
        .expect("preserved read future never completed");
    assert!(
        matches!(reply, CoreBluetoothReply::ReadResult(ref data) if *data == vec![1, 2, 3]),
        "unexpected reply: {reply:?}"
    );

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn rediscovery_errors_pending_read_of_removed_service() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let removed_service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let remaining_service_uuid = Uuid::from_u128(0x00001801_0000_1000_8000_00805f9b34fb);
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let characteristic_cbuuid = uuid_to_cbuuid(characteristic_uuid);
    let characteristic = unsafe {
        CBMutableCharacteristic::initWithType_properties_value_permissions(
            CBMutableCharacteristic::alloc(),
            &characteristic_cbuuid,
            CBCharacteristicProperties::Read,
            None,
            CBAttributePermissions::Readable,
        )
    };
    let characteristic: Retained<CBCharacteristic> = Retained::into_super(characteristic);
    let removed_service = unsafe {
        CBMutableService::initWithType_primary(
            CBMutableService::alloc(),
            &uuid_to_cbuuid(removed_service_uuid),
            true,
        )
    };
    let removed_service: Retained<CBService> = Retained::into_super(removed_service);
    internal.services.insert(
        removed_service_uuid,
        ServiceInternal {
            cbservice: removed_service,
            characteristics: HashMap::from([(
                characteristic_uuid,
                CharacteristicInternal::new(characteristic),
            )]),
            discovered: true,
        },
    );

    let read_future = CoreBluetoothReplyFuture::default();
    internal
        .services
        .get_mut(&removed_service_uuid)
        .unwrap()
        .characteristics
        .get_mut(&characteristic_uuid)
        .unwrap()
        .read_future_state
        .push_back(read_future.get_state_clone());

    let remaining_service = unsafe {
        CBMutableService::initWithType_primary(
            CBMutableService::alloc(),
            &uuid_to_cbuuid(remaining_service_uuid),
            true,
        )
    };
    let remaining_service: Retained<CBService> = Retained::into_super(remaining_service);
    internal.set_discovered_services(
        HashMap::from([(remaining_service_uuid, remaining_service)]),
        None,
    );

    assert!(!internal.services.contains_key(&removed_service_uuid));
    let reply = tokio::time::timeout(Duration::from_secs(1), read_future)
        .await
        .expect("removed service's pending read never completed");
    assert!(
        matches!(reply, CoreBluetoothReply::Err(_)),
        "unexpected reply: {reply:?}"
    );

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn drain_pending_operations_errors_service_and_characteristic_queues() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let characteristic_cbuuid = uuid_to_cbuuid(characteristic_uuid);
    let characteristic = unsafe {
        CBMutableCharacteristic::initWithType_properties_value_permissions(
            CBMutableCharacteristic::alloc(),
            &characteristic_cbuuid,
            CBCharacteristicProperties::Read,
            None,
            CBAttributePermissions::Readable,
        )
    };
    let characteristic: Retained<CBCharacteristic> = Retained::into_super(characteristic);
    let mut characteristic_internal = CharacteristicInternal::new(characteristic);

    let service = unsafe {
        CBMutableService::initWithType_primary(
            CBMutableService::alloc(),
            &uuid_to_cbuuid(service_uuid),
            true,
        )
    };
    let service: Retained<CBService> = Retained::into_super(service);

    let char_read = CoreBluetoothReplyFuture::default();
    let char_write = CoreBluetoothReplyFuture::default();
    characteristic_internal
        .read_future_state
        .push_back(char_read.get_state_clone());
    characteristic_internal
        .write_future_state
        .push_back(char_write.get_state_clone());

    internal.services.insert(
        service_uuid,
        ServiceInternal {
            cbservice: service,
            characteristics: HashMap::from([(characteristic_uuid, characteristic_internal)]),
            discovered: true,
        },
    );

    internal.drain_pending_operations("Peripheral cleared");

    for (name, future) in [("char_read", char_read), ("char_write", char_write)] {
        let reply = tokio::time::timeout(Duration::from_secs(1), future)
            .await
            .unwrap_or_else(|_| panic!("{name} did not drain"));
        assert!(
            matches!(reply, CoreBluetoothReply::Err(ref message) if message == "Peripheral cleared"),
            "{name}: unexpected drain reply: {reply:?}"
        );
    }

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn set_characteristics_removes_one_while_preserving_another() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let removed_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let kept_uuid = Uuid::from_u128(0x00002a00_0000_1000_8000_00805f9b34fb);
    let make_characteristic = |uuid: Uuid| -> Retained<CBCharacteristic> {
        let cbuuid = uuid_to_cbuuid(uuid);
        let characteristic = unsafe {
            CBMutableCharacteristic::initWithType_properties_value_permissions(
                CBMutableCharacteristic::alloc(),
                &cbuuid,
                CBCharacteristicProperties::Read,
                None,
                CBAttributePermissions::Readable,
            )
        };
        Retained::into_super(characteristic)
    };
    let service = unsafe {
        CBMutableService::initWithType_primary(
            CBMutableService::alloc(),
            &uuid_to_cbuuid(service_uuid),
            true,
        )
    };
    let service: Retained<CBService> = Retained::into_super(service);

    let removed_write = CoreBluetoothReplyFuture::default();
    let mut removed_characteristic = CharacteristicInternal::new(make_characteristic(removed_uuid));
    removed_characteristic
        .write_future_state
        .push_back(removed_write.get_state_clone());
    let kept_read = CoreBluetoothReplyFuture::default();
    let mut kept_characteristic = CharacteristicInternal::new(make_characteristic(kept_uuid));
    kept_characteristic.discovered = true;
    kept_characteristic
        .read_future_state
        .push_back(kept_read.get_state_clone());

    internal.services.insert(
        service_uuid,
        ServiceInternal {
            cbservice: service,
            characteristics: HashMap::from([
                (removed_uuid, removed_characteristic),
                (kept_uuid, kept_characteristic),
            ]),
            discovered: true,
        },
    );

    // Re-discovery only reports the characteristic being kept.
    internal.set_characteristics(
        service_uuid,
        HashMap::from([(kept_uuid, make_characteristic(kept_uuid))]),
        false,
    );

    let reply = tokio::time::timeout(Duration::from_secs(1), removed_write)
        .await
        .expect("removed characteristic's pending write never completed");
    assert!(matches!(reply, CoreBluetoothReply::Err(_)));

    let service = internal.services.get(&service_uuid).expect("service");
    assert!(!service.characteristics.contains_key(&removed_uuid));
    let kept = service
        .characteristics
        .get(&kept_uuid)
        .expect("kept characteristic preserved");
    assert!(
        !kept.discovered,
        "CB redrives descriptor discovery, so the gate must reset"
    );
    assert_eq!(
        kept.read_future_state.len(),
        1,
        "kept read future preserved"
    );

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn remove_services_errors_only_the_invalidated_services() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let removed_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let kept_uuid = Uuid::from_u128(0x00001801_0000_1000_8000_00805f9b34fb);

    let make_service = |uuid: Uuid| -> Retained<CBService> {
        let service = unsafe {
            CBMutableService::initWithType_primary(
                CBMutableService::alloc(),
                &uuid_to_cbuuid(uuid),
                true,
            )
        };
        Retained::into_super(service)
    };

    let removed_read = CoreBluetoothReplyFuture::default();
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
    let characteristic_cbuuid = uuid_to_cbuuid(characteristic_uuid);
    let mut removed_characteristic = CharacteristicInternal::new(unsafe {
        Retained::into_super(
            CBMutableCharacteristic::initWithType_properties_value_permissions(
                CBMutableCharacteristic::alloc(),
                &characteristic_cbuuid,
                CBCharacteristicProperties::Read,
                None,
                CBAttributePermissions::Readable,
            ),
        )
    });
    removed_characteristic
        .read_future_state
        .push_back(removed_read.get_state_clone());
    internal.services.insert(
        removed_uuid,
        ServiceInternal {
            cbservice: make_service(removed_uuid),
            characteristics: HashMap::from([(characteristic_uuid, removed_characteristic)]),
            discovered: true,
        },
    );
    internal.services.insert(
        kept_uuid,
        ServiceInternal {
            cbservice: make_service(kept_uuid),
            characteristics: HashMap::new(),
            discovered: true,
        },
    );

    internal.remove_services(&[removed_uuid], "Service invalidated; rediscovery required");

    assert!(!internal.services.contains_key(&removed_uuid));
    assert!(internal.services.contains_key(&kept_uuid));
    let reply = tokio::time::timeout(Duration::from_secs(1), removed_read)
        .await
        .expect("removed service's pending read never completed");
    assert!(matches!(reply, CoreBluetoothReply::Err(_)));

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn remove_services_reports_no_round_in_progress_for_stale_services() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let kept_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let removed_uuid = Uuid::from_u128(0x00001801_0000_1000_8000_00805f9b34fb);

    let make_service = |uuid: Uuid| -> Retained<CBService> {
        let service = unsafe {
            CBMutableService::initWithType_primary(
                CBMutableService::alloc(),
                &uuid_to_cbuuid(uuid),
                true,
            )
        };
        Retained::into_super(service)
    };

    // Both services are fully discovered, left over from a previous round.
    internal.services.insert(
        kept_uuid,
        ServiceInternal {
            cbservice: make_service(kept_uuid),
            characteristics: HashMap::new(),
            discovered: true,
        },
    );
    internal.services.insert(
        removed_uuid,
        ServiceInternal {
            cbservice: make_service(removed_uuid),
            characteristics: HashMap::new(),
            discovered: true,
        },
    );

    // discover_services() queues a future without resetting any
    // discovered flags; didDiscoverServices hasn't arrived yet.
    let mut discovery = CoreBluetoothReplyFuture::default();
    internal
        .services_discovered_future_state
        .push_back(discovery.get_state_clone());

    // didModifyServices arrives first and invalidates one service. This
    // mirrors on_services_modified: check_discovered() only runs when
    // remove_services reports a round was actually in progress.
    let was_mid_round =
        internal.remove_services(&[removed_uuid], "Service invalidated; rediscovery required");
    if was_mid_round {
        internal.check_discovered();
    }

    assert!(
        !was_mid_round,
        "the removed service was a stale leftover, not part of an in-progress round"
    );
    assert_pending(
        &mut discovery,
        "discovery must not complete with a stale leftover set",
    )
    .await;

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}

#[tokio::test]
async fn remove_services_drains_matching_write_without_response_queue() {
    let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
    let (peripheral, mut internal) = new_test_internal(peripheral_uuid);
    let removed_service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
    let kept_service_uuid = Uuid::from_u128(0x00001801_0000_1000_8000_00805f9b34fb);
    let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);

    let removed_pending = CoreBluetoothReplyFuture::default();
    let mut kept_pending = CoreBluetoothReplyFuture::default();
    internal
        .write_without_response_queue
        .push_back(PendingWriteWithoutResponse {
            service_uuid: removed_service_uuid,
            characteristic_uuid,
            data: vec![1],
            fut: removed_pending.get_state_clone(),
        });
    internal
        .write_without_response_queue
        .push_back(PendingWriteWithoutResponse {
            service_uuid: kept_service_uuid,
            characteristic_uuid,
            data: vec![2],
            fut: kept_pending.get_state_clone(),
        });

    internal.remove_services(
        &[removed_service_uuid],
        "Service invalidated; rediscovery required",
    );

    let reply = tokio::time::timeout(Duration::from_secs(1), removed_pending)
        .await
        .expect("removed service's queued write-without-response never completed");
    assert!(matches!(reply, CoreBluetoothReply::Err(_)));
    assert_eq!(
        internal.write_without_response_queue.len(),
        1,
        "the kept service's queued entry must survive"
    );
    assert_pending(
        &mut kept_pending,
        "kept service's queued write-without-response",
    )
    .await;

    std::mem::forget(internal);
    std::mem::forget(peripheral);
}
