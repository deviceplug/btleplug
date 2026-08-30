//! Shared test case bodies for integration tests.
//!
//! Each function contains the actual test logic, callable from both
//! desktop `#[tokio::test]` wrappers and Android JNI test harness.

use btleplug::api::Peripheral as _;

use super::gatt_uuids;
use super::peripheral_finder;

// ── Adapter capabilities ─────────────────────────────────────────────

pub async fn test_adapter_address() {
    use btleplug::api::{BDAddr, Central};

    let adapter = peripheral_finder::get_adapter().await;
    let result = adapter
        .adapter_address()
        .await
        .expect("Failed to get adapter address");
    match &result {
        Some(address) => assert_ne!(
            *address,
            BDAddr::default(),
            "adapter address must be nonzero"
        ),
        None => {
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            panic!("adapter address is unavailable on a supported desktop platform");
            #[cfg(any(target_vendor = "apple", target_os = "android"))]
            return;
        }
    }

    #[cfg(target_os = "linux")]
    if let Ok(expected) = std::env::var("BTLEPLUG_TEST_ADAPTER_ADDRESS") {
        let expected = expected
            .parse()
            .expect("invalid BTLEPLUG_TEST_ADAPTER_ADDRESS");
        assert_eq!(Some(expected), result);
    }
}

// ── Discovery ───────────────────────────────────────────────────────

pub async fn test_discover_peripheral_by_name() {
    use btleplug::api::Central;

    let adapter = peripheral_finder::get_adapter().await;
    let info = adapter
        .adapter_info()
        .await
        .expect("Failed to get adapter info");
    assert!(!info.is_empty(), "Adapter info should not be empty");
    let _state = adapter
        .adapter_state()
        .await
        .expect("Failed to get adapter state");

    let peripheral = peripheral_finder::find_and_connect().await;
    let props = peripheral.properties().await.unwrap().unwrap();
    let name = props.local_name.unwrap_or_default();
    let expected = std::env::var("BTLEPLUG_TEST_PERIPHERAL")
        .unwrap_or_else(|_| gatt_uuids::TEST_PERIPHERAL_NAME.to_string());
    assert_eq!(name, expected);
    peripheral.disconnect().await.unwrap();
}

#[cfg(target_os = "macos")]
pub async fn test_clear_peripherals_rediscovers_device() {
    use btleplug::api::{Central, CentralEvent, ScanFilter};
    use futures::StreamExt;
    use std::time::Duration;
    use tokio::time;

    let adapter = peripheral_finder::get_adapter().await;
    let peripheral_name = std::env::var("BTLEPLUG_TEST_PERIPHERAL")
        .unwrap_or_else(|_| gatt_uuids::TEST_PERIPHERAL_NAME.to_string());
    let mut events = adapter.events().await.unwrap();

    adapter.start_scan(ScanFilter::default()).await.unwrap();
    let peripheral = time::timeout(Duration::from_secs(15), async {
        loop {
            for peripheral in adapter.peripherals().await.unwrap() {
                if peripheral
                    .properties()
                    .await
                    .unwrap()
                    .is_some_and(|properties| {
                        properties.local_name.as_deref() == Some(&peripheral_name)
                    })
                {
                    return peripheral;
                }
            }
            let _ = events.next().await;
        }
    })
    .await
    .expect("timed out waiting for initial peripheral discovery");
    let peripheral_id = peripheral.id();

    adapter.stop_scan().await.unwrap();
    adapter.clear_peripherals().await.unwrap();
    assert!(
        adapter.peripherals().await.unwrap().is_empty(),
        "clear_peripherals returned before the public map was cleared"
    );

    adapter.start_scan(ScanFilter::default()).await.unwrap();
    time::timeout(Duration::from_secs(15), async {
        loop {
            if matches!(events.next().await, Some(CentralEvent::DeviceDiscovered(id)) if id == peripheral_id)
            {
                break;
            }
        }
    })
    .await
    .expect("timed out waiting for rediscovery after clear_peripherals");
    adapter.stop_scan().await.unwrap();

    assert!(
        adapter
            .peripherals()
            .await
            .unwrap()
            .iter()
            .any(|peripheral| peripheral.id() == peripheral_id),
        "rediscovered peripheral was not restored to the public map"
    );
}

pub async fn test_discover_services() {
    let peripheral = peripheral_finder::find_and_connect().await;
    let services = peripheral.services();
    let service_uuids: Vec<_> = services.iter().map(|s| s.uuid).collect();
    assert!(
        service_uuids.contains(&gatt_uuids::CONTROL_SERVICE),
        "Control Service not found in {:?}",
        service_uuids
    );
    assert!(
        service_uuids.contains(&gatt_uuids::READ_WRITE_SERVICE),
        "Read/Write Service not found"
    );
    assert!(
        service_uuids.contains(&gatt_uuids::NOTIFICATION_SERVICE),
        "Notification Service not found"
    );
    assert!(
        service_uuids.contains(&gatt_uuids::DESCRIPTOR_SERVICE),
        "Descriptor Service not found"
    );
    peripheral.disconnect().await.unwrap();
}

pub async fn test_discover_characteristics() {
    let peripheral = peripheral_finder::find_and_connect().await;
    let chars = peripheral.characteristics();
    let char_uuids: Vec<_> = chars.iter().map(|c| c.uuid).collect();
    assert!(char_uuids.contains(&gatt_uuids::CONTROL_POINT));
    assert!(char_uuids.contains(&gatt_uuids::STATIC_READ));
    assert!(char_uuids.contains(&gatt_uuids::NOTIFY_CHAR));
    assert!(char_uuids.contains(&gatt_uuids::DESCRIPTOR_TEST_CHAR));
    peripheral.disconnect().await.unwrap();
}

pub async fn test_scan_filter_by_service_uuid() {
    use btleplug::api::{Central, ScanFilter};
    use std::time::Duration;
    use tokio::time;

    let adapter = peripheral_finder::get_adapter().await;
    adapter
        .start_scan(ScanFilter {
            services: vec![gatt_uuids::CONTROL_SERVICE],
        })
        .await
        .unwrap();
    time::sleep(Duration::from_secs(5)).await;
    let peripherals = adapter.peripherals().await.unwrap();
    adapter.stop_scan().await.unwrap();
    assert!(
        !peripherals.is_empty(),
        "No peripherals found with Control Service UUID filter"
    );
}

pub async fn test_advertisement_manufacturer_data() {
    use btleplug::api::{Central, CentralEvent, ScanFilter};
    use futures::StreamExt;
    use std::time::Duration;
    use tokio::time;

    let adapter = peripheral_finder::get_adapter().await;
    let mut events = adapter.events().await.unwrap();
    adapter.start_scan(ScanFilter::default()).await.unwrap();

    let mut found_manufacturer_data = false;
    let timeout = time::sleep(Duration::from_secs(10));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            Some(event) = events.next() => {
                if let CentralEvent::ManufacturerDataAdvertisement { manufacturer_data, .. } = event {
                    if manufacturer_data.contains_key(&gatt_uuids::MANUFACTURER_COMPANY_ID) {
                        found_manufacturer_data = true;
                        break;
                    }
                }
            }
            _ = &mut timeout => break,
        }
    }

    adapter.stop_scan().await.unwrap();
    assert!(
        found_manufacturer_data,
        "Did not receive ManufacturerDataAdvertisement with company ID 0xFFFF"
    );
}

pub async fn test_advertisement_services() {
    use btleplug::api::{Central, CentralEvent, ScanFilter};
    use futures::StreamExt;
    use std::time::Duration;
    use tokio::time;

    let adapter = peripheral_finder::get_adapter().await;
    let mut events = adapter.events().await.unwrap();
    adapter.start_scan(ScanFilter::default()).await.unwrap();

    let mut found_services = false;
    let timeout = time::sleep(Duration::from_secs(10));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            Some(event) = events.next() => {
                if let CentralEvent::ServicesAdvertisement { services, .. } = event {
                    if services.contains(&gatt_uuids::CONTROL_SERVICE) {
                        found_services = true;
                        break;
                    }
                }
            }
            _ = &mut timeout => break,
        }
    }

    adapter.stop_scan().await.unwrap();
    assert!(
        found_services,
        "Did not receive ServicesAdvertisement with Control Service UUID"
    );
}

// ── Retrieval ──────────────────────────────────────────────────────

pub async fn test_retrieve_peripherals_not_supported() {
    use btleplug::api::{Central, RetrievePeripheralsOptions};

    let adapter = peripheral_finder::get_adapter().await;
    let error = adapter
        .retrieve_peripherals(RetrievePeripheralsOptions::default())
        .await
        .expect_err("retrieval without selectors should not be supported on Android");
    assert!(matches!(
        error,
        btleplug::Error::NotSupported(operation) if operation == "retrieve_peripherals"
    ));
}

pub async fn test_retrieve_connected_peripheral_by_service() {
    use btleplug::api::{Central, RetrievePeripheralsOptions};

    let adapter = peripheral_finder::get_adapter().await;
    let expected = peripheral_finder::find_and_connect().await;
    let expected_id = expected.id();
    let retrieved = adapter
        .retrieve_peripherals(RetrievePeripheralsOptions {
            identifiers: None,
            services: Some(vec![gatt_uuids::CONTROL_SERVICE]),
        })
        .await
        .expect("retrieval by service should be supported on desktop backends");
    assert!(
        retrieved
            .iter()
            .any(|peripheral| peripheral.id() == expected_id),
        "connected test peripheral was not returned by service retrieval"
    );
    expected.disconnect().await.unwrap();
}

// ── Connection ──────────────────────────────────────────────────────

pub async fn test_connect_and_disconnect() {
    use std::time::Duration;
    use tokio::time;

    let peripheral = peripheral_finder::find_and_connect().await;
    assert!(peripheral.is_connected().await.unwrap());
    println!("Disconnecting");
    peripheral.disconnect().await.unwrap();
    println!("Disconnected");
    time::sleep(Duration::from_millis(500)).await;
    assert!(!peripheral.is_connected().await.unwrap());
    println!("Waiting on is connected update?");
}

pub async fn test_reconnect_after_disconnect() {
    use std::time::Duration;
    use tokio::time;

    let peripheral = peripheral_finder::find_and_connect().await;
    assert!(peripheral.is_connected().await.unwrap());
    peripheral.disconnect().await.unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(!peripheral.is_connected().await.unwrap());
    peripheral.connect().await.unwrap();
    assert!(peripheral.is_connected().await.unwrap());
    peripheral.discover_services().await.unwrap();
    assert!(!peripheral.services().is_empty());
    peripheral.disconnect().await.unwrap();
}

pub async fn test_peripheral_triggered_disconnect() {
    use std::time::Duration;
    use tokio::time;

    let peripheral = peripheral_finder::find_and_connect().await;
    assert!(peripheral.is_connected().await.unwrap());
    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_TRIGGER_DISCONNECT).await;
    time::sleep(Duration::from_secs(2)).await;
    assert!(
        !peripheral.is_connected().await.unwrap(),
        "Peripheral should have disconnected us"
    );
}

// ── Read / Write ────────────────────────────────────────────────────

pub async fn test_read_static_value() {
    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::STATIC_READ);
    let value = peripheral.read(&char).await.unwrap();
    assert_eq!(
        value,
        gatt_uuids::STATIC_READ_VALUE,
        "Static read should return [0x01, 0x02, 0x03, 0x04]"
    );
    peripheral.disconnect().await.unwrap();
}

pub async fn test_read_counter_increments() {
    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::COUNTER_READ);
    let first = peripheral.read(&char).await.unwrap();
    let second = peripheral.read(&char).await.unwrap();
    let first_val = u32::from_le_bytes(first[..4].try_into().unwrap());
    let second_val = u32::from_le_bytes(second[..4].try_into().unwrap());
    assert!(
        second_val > first_val,
        "Counter should increment: first={}, second={}",
        first_val,
        second_val
    );
    peripheral.disconnect().await.unwrap();
}

pub async fn test_write_with_response() {
    use btleplug::api::WriteType;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::WRITE_WITH_RESPONSE);
    let data = vec![0xAA, 0xBB, 0xCC];
    peripheral
        .write(&char, &data, WriteType::WithResponse)
        .await
        .unwrap();
    peripheral.disconnect().await.unwrap();
}

pub async fn test_write_without_response() {
    use btleplug::api::WriteType;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::WRITE_WITHOUT_RESPONSE);
    let data = vec![0x11, 0x22, 0x33];
    peripheral
        .write(&char, &data, WriteType::WithoutResponse)
        .await
        .unwrap();
    peripheral.disconnect().await.unwrap();
}

pub async fn test_write_without_response_burst() {
    use btleplug::api::WriteType;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::WRITE_WITHOUT_RESPONSE);

    // Send a burst of writes to exercise flow control. Without proper
    // canSendWriteWithoutResponse handling, later writes would be silently
    // dropped by CoreBluetooth.
    let num_writes = 50;
    for i in 0u8..num_writes {
        let data = vec![i; 20];
        peripheral
            .write(&char, &data, WriteType::WithoutResponse)
            .await
            .expect(&format!("write-without-response #{} should succeed", i));
    }
    peripheral.disconnect().await.unwrap();
}

pub async fn test_read_write_roundtrip() {
    use btleplug::api::WriteType;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::READ_WRITE);
    let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
    peripheral
        .write(&char, &data, WriteType::WithResponse)
        .await
        .unwrap();
    let read_back = peripheral.read(&char).await.unwrap();
    assert_eq!(read_back, data, "Read-back should match written data");
    peripheral.disconnect().await.unwrap();
}

pub async fn test_long_value_read_write() {
    use btleplug::api::WriteType;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::LONG_VALUE);
    let data: Vec<u8> = (0..200).map(|i| (i % 256) as u8).collect();
    peripheral
        .write(&char, &data, WriteType::WithResponse)
        .await
        .unwrap();
    let read_back = peripheral.read(&char).await.unwrap();
    assert_eq!(
        read_back, data,
        "Long value read-back should match written data"
    );
    peripheral.disconnect().await.unwrap();
}

pub async fn test_characteristic_properties() {
    use btleplug::api::CharPropFlags;

    let peripheral = peripheral_finder::find_and_connect().await;
    let static_read = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::STATIC_READ);
    assert!(
        static_read.properties.contains(CharPropFlags::READ),
        "Static Read should have READ property"
    );
    let write_char =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::WRITE_WITH_RESPONSE);
    assert!(
        write_char.properties.contains(CharPropFlags::WRITE),
        "Write With Response should have WRITE property"
    );
    let write_no_resp =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::WRITE_WITHOUT_RESPONSE);
    assert!(
        write_no_resp
            .properties
            .contains(CharPropFlags::WRITE_WITHOUT_RESPONSE),
        "Write Without Response should have WRITE_WITHOUT_RESPONSE property"
    );
    peripheral.disconnect().await.unwrap();
}

// ── Notifications ───────────────────────────────────────────────────

pub async fn test_subscribe_and_receive_notifications() {
    use futures::StreamExt;
    use std::time::Duration;
    use tokio::time;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::NOTIFY_CHAR);
    let mut stream = peripheral.notifications().await.unwrap();
    peripheral.subscribe(&char).await.unwrap();
    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_START_NOTIFICATIONS).await;

    let mut received = Vec::new();
    let timeout = time::sleep(Duration::from_secs(5));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            Some(notification) = stream.next() => {
                if notification.uuid == gatt_uuids::NOTIFY_CHAR {
                    received.push(notification);
                    if received.len() >= 3 {
                        break;
                    }
                }
            }
            _ = &mut timeout => break,
        }
    }

    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_STOP_NOTIFICATIONS).await;
    peripheral.unsubscribe(&char).await.unwrap();

    assert!(
        received.len() >= 3,
        "Expected at least 3 notifications, got {}",
        received.len()
    );
    for notif in &received {
        assert_eq!(notif.service_uuid, gatt_uuids::NOTIFICATION_SERVICE);
    }
    peripheral.disconnect().await.unwrap();
}

pub async fn test_subscribe_and_receive_indications() {
    use futures::StreamExt;
    use std::time::Duration;
    use tokio::time;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::INDICATE_CHAR);
    let mut stream = peripheral.notifications().await.unwrap();
    peripheral.subscribe(&char).await.unwrap();
    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_START_NOTIFICATIONS).await;

    let mut received = Vec::new();
    let timeout = time::sleep(Duration::from_secs(5));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            Some(notification) = stream.next() => {
                if notification.uuid == gatt_uuids::INDICATE_CHAR {
                    received.push(notification);
                    if received.len() >= 2 {
                        break;
                    }
                }
            }
            _ = &mut timeout => break,
        }
    }

    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_STOP_NOTIFICATIONS).await;
    peripheral.unsubscribe(&char).await.unwrap();

    assert!(
        received.len() >= 2,
        "Expected at least 2 indications, got {}",
        received.len()
    );
    peripheral.disconnect().await.unwrap();
}

pub async fn test_unsubscribe_stops_notifications() {
    use futures::StreamExt;
    use std::time::Duration;
    use tokio::time;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::NOTIFY_CHAR);
    let mut stream = peripheral.notifications().await.unwrap();
    peripheral.subscribe(&char).await.unwrap();
    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_START_NOTIFICATIONS).await;

    let timeout = time::sleep(Duration::from_secs(3));
    tokio::pin!(timeout);
    let mut got_one = false;
    loop {
        tokio::select! {
            Some(n) = stream.next() => {
                if n.uuid == gatt_uuids::NOTIFY_CHAR {
                    got_one = true;
                    break;
                }
            }
            _ = &mut timeout => break,
        }
    }
    assert!(got_one, "Should have received at least one notification");

    peripheral.unsubscribe(&char).await.unwrap();
    time::sleep(Duration::from_secs(2)).await;

    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_STOP_NOTIFICATIONS).await;
    peripheral.disconnect().await.unwrap();
}

pub async fn test_configurable_notification_payload() {
    use futures::StreamExt;
    use std::time::Duration;
    use tokio::time;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let config_char =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::CONFIGURABLE_NOTIFY);
    let control_point =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::CONTROL_POINT);

    let mut cmd = vec![gatt_uuids::CMD_SET_NOTIFICATION_PAYLOAD];
    cmd.extend_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE]);
    peripheral
        .write(&control_point, &cmd, btleplug::api::WriteType::WithResponse)
        .await
        .unwrap();

    let mut stream = peripheral.notifications().await.unwrap();
    peripheral.subscribe(&config_char).await.unwrap();
    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_START_NOTIFICATIONS).await;

    let timeout = time::sleep(Duration::from_secs(5));
    tokio::pin!(timeout);
    let mut matching = false;

    loop {
        tokio::select! {
            Some(n) = stream.next() => {
                if n.uuid == gatt_uuids::CONFIGURABLE_NOTIFY
                    && n.value == vec![0xCA, 0xFE, 0xBA, 0xBE]
                {
                    matching = true;
                    break;
                }
            }
            _ = &mut timeout => break,
        }
    }

    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_STOP_NOTIFICATIONS).await;
    peripheral.unsubscribe(&config_char).await.unwrap();
    assert!(
        matching,
        "Should receive notification with custom payload [0xCA, 0xFE, 0xBA, 0xBE]"
    );
    peripheral.disconnect().await.unwrap();
}

// ── Descriptors ─────────────────────────────────────────────────────

pub async fn test_read_only_descriptor() {
    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let descriptor = super::find_descriptor(
        &peripheral,
        gatt_uuids::DESCRIPTOR_TEST_CHAR,
        gatt_uuids::READ_ONLY_DESCRIPTOR,
    );
    let value = peripheral.read_descriptor(&descriptor).await.unwrap();
    assert_eq!(
        value,
        vec![0xDE, 0xAD, 0xBE, 0xEF],
        "Read-only descriptor should return fixed value"
    );
    peripheral.disconnect().await.unwrap();
}

pub async fn test_read_write_descriptor_roundtrip() {
    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let descriptor = super::find_descriptor(
        &peripheral,
        gatt_uuids::DESCRIPTOR_TEST_CHAR,
        gatt_uuids::READ_WRITE_DESCRIPTOR,
    );
    let data = vec![0x42, 0x43, 0x44];
    peripheral
        .write_descriptor(&descriptor, &data)
        .await
        .unwrap();
    let read_back = peripheral.read_descriptor(&descriptor).await.unwrap();
    assert_eq!(
        read_back, data,
        "Descriptor read-back should match written data"
    );
    peripheral.disconnect().await.unwrap();
}

pub async fn test_descriptor_discovery() {
    let peripheral = peripheral_finder::find_and_connect().await;
    let char =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::DESCRIPTOR_TEST_CHAR);
    let descriptor_uuids: Vec<_> = char.descriptors.iter().map(|d| d.uuid).collect();
    assert!(
        descriptor_uuids.contains(&gatt_uuids::READ_ONLY_DESCRIPTOR),
        "Read-only descriptor not found. Found: {:?}",
        descriptor_uuids
    );
    assert!(
        descriptor_uuids.contains(&gatt_uuids::READ_WRITE_DESCRIPTOR),
        "Read/write descriptor not found. Found: {:?}",
        descriptor_uuids
    );
    peripheral.disconnect().await.unwrap();
}

// ── Device Info ─────────────────────────────────────────────────────

pub async fn test_mtu_after_service_discovery() {
    let peripheral = peripheral_finder::find_and_connect().await;
    let mtu = peripheral.mtu();

    assert!(
        mtu >= 23,
        "MTU should be at least 23 (default), got {}",
        mtu
    );
    peripheral.disconnect().await.unwrap();
}

pub async fn test_read_rssi() {
    let peripheral = peripheral_finder::find_and_connect().await;
    match peripheral.read_rssi().await {
        Ok(rssi) => {
            assert!(
                rssi < 0 && rssi > -120,
                "RSSI should be between -120 and 0 dBm, got {}",
                rssi
            );
        }
        Err(btleplug::Error::NotSupported(_)) => {}
        Err(e) => {
            panic!("Unexpected error from read_rssi: {:?}", e);
        }
    }
    peripheral.disconnect().await.unwrap();
}

pub async fn test_properties_contain_peripheral_info() {
    let peripheral = peripheral_finder::find_and_connect().await;

    let id_str = format!("{:?}", peripheral.id());
    assert!(!id_str.is_empty(), "Peripheral ID should not be empty");

    let addr_str = format!("{:?}", peripheral.address());
    assert!(
        !addr_str.is_empty(),
        "Peripheral address should not be empty"
    );

    let props = peripheral
        .properties()
        .await
        .unwrap()
        .expect("properties should be available");

    let expected_name = std::env::var("BTLEPLUG_TEST_PERIPHERAL")
        .unwrap_or_else(|_| gatt_uuids::TEST_PERIPHERAL_NAME.to_string());
    assert_eq!(props.local_name.as_deref(), Some(expected_name.as_str()),);
    assert!(
        props
            .manufacturer_data
            .contains_key(&gatt_uuids::MANUFACTURER_COMPANY_ID),
        "Properties should contain manufacturer data with company ID 0xFFFF"
    );
    assert!(props.rssi.is_some(), "RSSI from scan should be present");
    assert!(
        props.tx_power_level.is_some(),
        "TX Power Level should be present in advertisement properties"
    );
    #[cfg(target_vendor = "apple")]
    assert_eq!(
        props.appearance, None,
        "CoreBluetooth does not expose GAP Appearance advertising data"
    );
    #[cfg(not(target_vendor = "apple"))]
    assert_eq!(
        props.appearance,
        Some(gatt_uuids::TEST_APPEARANCE),
        "Properties should contain the advertised GAP Appearance"
    );
    peripheral.disconnect().await.unwrap();
}

pub async fn test_connection_parameters() {
    let peripheral = peripheral_finder::find_and_connect().await;
    match peripheral.connection_parameters().await {
        Ok(Some(params)) => {
            assert!(
                params.interval_us >= 7_500 && params.interval_us <= 4_000_000,
                "Connection interval out of range: {} us",
                params.interval_us
            );
            assert!(
                params.latency <= 499,
                "Latency out of range: {}",
                params.latency
            );
            assert!(
                params.supervision_timeout_us >= 100_000
                    && params.supervision_timeout_us <= 32_000_000,
                "Supervision timeout out of range: {} us",
                params.supervision_timeout_us
            );
        }
        Ok(None) => {}
        Err(btleplug::Error::NotSupported(_)) => {}
        Err(e) => {
            panic!("Unexpected error from connection_parameters: {:?}", e);
        }
    }
    peripheral.disconnect().await.unwrap();
}

pub async fn test_request_connection_parameters() {
    use btleplug::api::ConnectionParameterPreset;

    let peripheral = peripheral_finder::find_and_connect().await;
    match peripheral
        .request_connection_parameters(ConnectionParameterPreset::ThroughputOptimized)
        .await
    {
        Ok(()) => {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if let Ok(Some(params)) = peripheral.connection_parameters().await {
                assert!(
                    params.interval_us > 0,
                    "Connection interval should be positive after update"
                );
            }
        }
        Err(btleplug::Error::NotSupported(_)) => {}
        Err(e) => {
            panic!(
                "Unexpected error from request_connection_parameters: {:?}",
                e
            );
        }
    }
    peripheral.disconnect().await.unwrap();
}
