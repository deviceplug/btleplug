//! Shared test case bodies for integration tests.
//!
//! Each function contains the actual test logic, callable from both
//! desktop `#[tokio::test]` wrappers and Android JNI test harness.

#![allow(dead_code)]

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
    use futures::{FutureExt, StreamExt};
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
    // Drop events emitted before the clear, including the initial DeviceDiscovered.
    while let Some(Some(_)) = events.next().now_or_never() {}

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

/// Covers #489: CoreBluetooth disconnects a connected peripheral as part of
/// `clear_peripherals()` but does not emit `DeviceDisconnected` for it (see
/// the `Central::clear_peripherals` doc). Proves the link actually dropped
/// via rediscovery -- the firmware only re-advertises once the connection is
/// really gone -- and that the rediscovered handle still works.
#[cfg(target_os = "macos")]
pub async fn test_clear_peripherals_disconnects_connected_peripheral() {
    use btleplug::api::{Central, CentralEvent, Peripheral as _};
    use std::time::Duration;

    let peripheral = peripheral_finder::find_and_connect().await;
    let target_id = peripheral.id();
    let mut events = peripheral_finder::spawn_event_collector().await;

    let adapter = peripheral_finder::get_adapter().await;
    tokio::time::timeout(Duration::from_secs(10), adapter.clear_peripherals())
        .await
        .expect("clear_peripherals() timed out")
        .expect("clear_peripherals() failed");

    assert!(
        !adapter
            .peripherals()
            .await
            .unwrap()
            .iter()
            .any(|p| p.id() == target_id),
        "cleared peripheral is still in the public map"
    );

    // Bound the negative assertion (no DeviceDisconnected) by the positive
    // rediscovery event rather than a sleep.
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match events.recv().await {
                Some(CentralEvent::DeviceDisconnected(id)) if id == target_id => panic!(
                    "DeviceDisconnected(test peripheral) observed -- clear_peripherals() must \
                     not emit it on CoreBluetooth"
                ),
                Some(CentralEvent::DeviceDiscovered(id)) if id == target_id => break,
                Some(_) => continue,
                None => panic!("event collector channel closed while waiting for rediscovery"),
            }
        }
    })
    .await
    .expect("timed out waiting for rediscovery after clear_peripherals");

    let rediscovered = adapter
        .peripheral(&target_id)
        .await
        .expect("rediscovered peripheral not returned by peripheral()");

    tokio::time::timeout(Duration::from_secs(10), rediscovered.connect())
        .await
        .expect("connect() on rediscovered peripheral timed out")
        .expect("connect() on rediscovered peripheral failed");

    tokio::time::timeout(Duration::from_secs(10), rediscovered.discover_services())
        .await
        .expect("discover_services() on rediscovered peripheral timed out")
        .expect("discover_services() on rediscovered peripheral failed");

    let char = peripheral_finder::find_characteristic(&rediscovered, gatt_uuids::STATIC_READ);
    let value = tokio::time::timeout(Duration::from_secs(10), rediscovered.read(&char))
        .await
        .expect("read(STATIC_READ) on rediscovered peripheral timed out")
        .expect("read(STATIC_READ) on rediscovered peripheral failed");
    assert_eq!(
        value,
        gatt_uuids::STATIC_READ_VALUE,
        "Static read should return [0x01, 0x02, 0x03, 0x04]"
    );

    rediscovered.disconnect().await.unwrap();
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

/// Windows and Android construct a `Peripheral` from a bare id, so the returned handle must be
/// usable. BlueZ and CoreBluetooth return `Error::NotSupported`; only the variant is asserted
/// because its message does not follow the operation-name convention `retrieve_peripherals` uses.
pub async fn test_add_peripheral_by_address() {
    use btleplug::api::{Central, Peripheral as _};
    use std::time::Duration;

    let peripheral = peripheral_finder::find_and_connect().await;
    let id = peripheral.id();
    let adapter = peripheral_finder::get_adapter().await;

    #[cfg(any(target_os = "linux", target_vendor = "apple"))]
    {
        let result = tokio::time::timeout(Duration::from_secs(10), adapter.add_peripheral(&id))
            .await
            .expect("add_peripheral() timed out");
        assert!(
            matches!(result, Err(btleplug::Error::NotSupported(_))),
            "expected Error::NotSupported from add_peripheral(), got {:?}",
            result
        );
        peripheral.disconnect().await.unwrap();
    }

    #[cfg(any(target_os = "windows", target_os = "android"))]
    {
        peripheral.disconnect().await.unwrap();

        // The background scan may re-add the peripheral before add_peripheral() runs; both
        // backends return an existing map entry before constructing one, so either way the
        // returned handle is valid.
        adapter.clear_peripherals().await.unwrap();

        let added = tokio::time::timeout(Duration::from_secs(10), adapter.add_peripheral(&id))
            .await
            .expect("add_peripheral() timed out")
            .expect("add_peripheral() should succeed");
        assert_eq!(
            added.id(),
            id,
            "add_peripheral() returned a peripheral with a different id"
        );

        tokio::time::timeout(Duration::from_secs(10), added.connect())
            .await
            .expect("connect() on add_peripheral()'s handle timed out")
            .expect("connect() on add_peripheral()'s handle failed");
        assert!(added.is_connected().await.unwrap());

        tokio::time::timeout(Duration::from_secs(10), added.discover_services())
            .await
            .expect("discover_services() on add_peripheral()'s handle timed out")
            .expect("discover_services() on add_peripheral()'s handle failed");

        let char = peripheral_finder::find_characteristic(&added, gatt_uuids::STATIC_READ);
        let value = tokio::time::timeout(Duration::from_secs(10), added.read(&char))
            .await
            .expect("read(STATIC_READ) on add_peripheral()'s handle timed out")
            .expect("read(STATIC_READ) on add_peripheral()'s handle failed");
        assert_eq!(
            value,
            gatt_uuids::STATIC_READ_VALUE,
            "Static read should return [0x01, 0x02, 0x03, 0x04]"
        );

        added.disconnect().await.unwrap();
    }
}

/// Android does not support retrieval (covered by `test_retrieve_peripherals_not_supported`).
#[cfg(not(target_os = "android"))]
pub async fn test_retrieve_connected_peripheral_by_identifier() {
    use btleplug::api::{Central, Peripheral as _, RetrievePeripheralsOptions};

    let adapter = peripheral_finder::get_adapter().await;
    let expected = peripheral_finder::find_and_connect().await;
    let expected_id = expected.id();

    let retrieved = adapter
        .retrieve_peripherals(RetrievePeripheralsOptions {
            identifiers: Some(vec![expected_id.clone()]),
            services: None,
        })
        .await
        .expect("retrieval by identifier should be supported on desktop backends");
    let matched = retrieved
        .iter()
        .find(|peripheral| peripheral.id() == expected_id)
        .expect("connected test peripheral was not returned by identifier retrieval");
    assert!(
        matched.is_connected().await.unwrap(),
        "retrieved peripheral should report connected"
    );

    expected.disconnect().await.unwrap();
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
    use btleplug::api::CentralEvent;
    use std::time::Duration;

    // Start the collector before find_and_connect() so the DeviceConnected
    // event for this peripheral can't be lost to broadcast-channel lag
    // (see peripheral_finder::spawn_event_collector).
    let mut events = peripheral_finder::spawn_event_collector().await;

    let peripheral = peripheral_finder::find_and_connect().await;
    assert!(peripheral.is_connected().await.unwrap());
    let target_id = peripheral.id();

    // On BlueZ, events() synthesises a DeviceConnected for every
    // already-connected device the moment the stream opens, and BlueZ keeps
    // LE links alive after a process exits -- so a prior test that panicked
    // mid-connection can leave a stale DeviceConnected(id) at the head of
    // this collector, followed by a DeviceDisconnected(id) from this test's
    // own ensure_clean_state() and then the real DeviceConnected(id).
    // wait_for_connected() discards that stale connect/disconnect pair and
    // only returns once it has seen a DeviceConnected(id) with nothing after
    // it but silence (see peripheral_finder::wait_for_connected).
    peripheral_finder::wait_for_connected(&mut events, &target_id, Duration::from_secs(15)).await;

    println!("Disconnecting");
    peripheral.disconnect().await.unwrap();
    println!("Disconnected");

    peripheral_finder::wait_for_event(
        &mut events,
        Duration::from_secs(15),
        "DeviceDisconnected(test peripheral)",
        |event| matches!(event, CentralEvent::DeviceDisconnected(id) if *id == target_id),
    )
    .await;
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
    use btleplug::api::CentralEvent;
    use std::time::Duration;

    // Start the collector before find_and_connect() so the DeviceConnected
    // event for this peripheral can't be lost to broadcast-channel lag
    // (see peripheral_finder::spawn_event_collector).
    let mut events = peripheral_finder::spawn_event_collector().await;

    let peripheral = peripheral_finder::find_and_connect().await;
    assert!(peripheral.is_connected().await.unwrap());
    let target_id = peripheral.id();

    // See test_connect_and_disconnect: a stale DeviceConnected/Disconnected
    // pair left over from ensure_clean_state() cleaning up a prior test's
    // lingering connection would make a bare DeviceConnected wait match too
    // early, so the later DeviceDisconnected wait below would then match the
    // buffered disconnect from that stale pair instead of the firmware's
    // 500ms delayed disconnect. wait_for_connected() discards the stale pair
    // (see peripheral_finder::wait_for_connected).
    peripheral_finder::wait_for_connected(&mut events, &target_id, Duration::from_secs(15)).await;

    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_TRIGGER_DISCONNECT).await;

    peripheral_finder::wait_for_event(
        &mut events,
        Duration::from_secs(10),
        "DeviceDisconnected(test peripheral)",
        |event| matches!(event, CentralEvent::DeviceDisconnected(id) if *id == target_id),
    )
    .await;
    assert!(
        !peripheral.is_connected().await.unwrap(),
        "Peripheral should have disconnected us"
    );
}

/// Covers #484: connect() must succeed after a peripheral-triggered
/// disconnect with no explicit disconnect() call first, and a subsequent
/// connect() while already connected must return Ok promptly (the fast path).
pub async fn test_reconnect_after_peripheral_triggered_disconnect() {
    use btleplug::api::CentralEvent;
    use std::time::Duration;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    assert!(peripheral.is_connected().await.unwrap());
    let target_id = peripheral.id();

    // Start the collector after reset_peripheral() -- only the disconnect
    // and rediscovery events matter here, and reset_peripheral() doesn't
    // disconnect.
    let mut events = peripheral_finder::spawn_event_collector().await;

    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_TRIGGER_DISCONNECT).await;

    peripheral_finder::wait_for_event(
        &mut events,
        Duration::from_secs(10),
        "DeviceDisconnected(test peripheral)",
        |event| matches!(event, CentralEvent::DeviceDisconnected(id) if *id == target_id),
    )
    .await;
    assert!(!peripheral.is_connected().await.unwrap());

    peripheral_finder::wait_for_rediscovery(&mut events, target_id).await;

    // #484: connect() without an explicit disconnect() first.
    tokio::time::timeout(Duration::from_secs(10), peripheral.connect())
        .await
        .expect("connect() after peripheral-triggered disconnect timed out")
        .expect("connect() after peripheral-triggered disconnect failed");
    assert!(peripheral.is_connected().await.unwrap());

    tokio::time::timeout(Duration::from_secs(10), peripheral.discover_services())
        .await
        .expect("discover_services() timed out")
        .expect("discover_services() failed");

    let char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::STATIC_READ);
    let value = tokio::time::timeout(Duration::from_secs(10), peripheral.read(&char))
        .await
        .expect("read(STATIC_READ) timed out")
        .expect("read(STATIC_READ) failed");
    assert_eq!(
        value,
        gatt_uuids::STATIC_READ_VALUE,
        "Static read should return [0x01, 0x02, 0x03, 0x04]"
    );

    // #484 fast path: connect() while already connected must return Ok
    // within 2 seconds instead of re-running the full connection sequence.
    let result = tokio::time::timeout(Duration::from_secs(2), peripheral.connect()).await;
    assert!(
        matches!(result, Ok(Ok(()))),
        "connect() while already connected did not return Ok within 2s: {:?}",
        result
    );

    peripheral.disconnect().await.unwrap();
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
    use std::time::Duration;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::WRITE_WITHOUT_RESPONSE);
    let log_char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::WRITE_LOG_CHAR);

    // Send a burst of writes to exercise flow control. Without proper
    // canSendWriteWithoutResponse handling, later writes would be silently
    // dropped or reordered by CoreBluetooth (#464). The first byte of each
    // payload is a sequence number the firmware checks for ordering.
    let num_writes = 50u8;
    for i in 0..num_writes {
        let data = vec![i; 20];
        peripheral
            .write(&char, &data, WriteType::WithoutResponse)
            .await
            .expect(&format!("write-without-response #{} should succeed", i));
    }

    // Poll WRITE_LOG_CHAR ([count_lo, count_hi, last_seq, out_of_order]) until
    // the firmware has observed all 50 writes.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut last: Option<[u8; 4]> = None;
    let (count, last_seq, out_of_order) = loop {
        let raw = tokio::time::timeout(Duration::from_secs(10), peripheral.read(&log_char))
            .await
            .unwrap_or_else(|_| panic!("read(WRITE_LOG_CHAR) timed out; last observed {last:?}"))
            .expect("read(WRITE_LOG_CHAR) should succeed");
        assert_eq!(raw.len(), 4, "WRITE_LOG_CHAR returned {raw:?}");
        let observed = [raw[0], raw[1], raw[2], raw[3]];
        last = Some(observed);
        let count = u16::from_le_bytes([observed[0], observed[1]]);
        if count >= u16::from(num_writes) {
            break (count, observed[2], observed[3]);
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "WRITE_LOG_CHAR did not reach count {num_writes} within 2s; last observed {last:?}"
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    assert_eq!(count, u16::from(num_writes), "write count mismatch");
    assert_eq!(last_seq, num_writes - 1, "last_seq mismatch");
    assert_eq!(out_of_order, 0, "writes were observed out of order");

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

/// Covers #492: GATT errors on read/write must surface as `Err`, and the link must stay usable.
pub async fn test_gatt_error_status_is_reported() {
    use btleplug::api::WriteType;
    use std::time::Duration;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::ERROR_CHAR);

    let read_result = tokio::time::timeout(Duration::from_secs(10), peripheral.read(&char))
        .await
        .expect("read(ERROR_CHAR) timed out");
    assert!(
        read_result.is_err(),
        "expected an error reading ERROR_CHAR, got {:?}",
        read_result
    );

    let write_result = tokio::time::timeout(
        Duration::from_secs(10),
        peripheral.write(&char, &[0x00], WriteType::WithResponse),
    )
    .await
    .expect("write(ERROR_CHAR) timed out");
    assert!(
        write_result.is_err(),
        "expected an error writing ERROR_CHAR, got {:?}",
        write_result
    );

    let static_char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::STATIC_READ);
    let value = tokio::time::timeout(Duration::from_secs(10), peripheral.read(&static_char))
        .await
        .expect("follow-up read(STATIC_READ) timed out")
        .expect("follow-up read(STATIC_READ) failed");
    assert_eq!(
        value,
        gatt_uuids::STATIC_READ_VALUE,
        "Static read should return [0x01, 0x02, 0x03, 0x04]"
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

    // Discard notifications that were already queued before unsubscribe. Stop
    // draining once the stream is briefly quiet, but never wait more than
    // three seconds in case the peripheral keeps notifying continuously.
    let drain_deadline = time::Instant::now() + Duration::from_secs(3);
    loop {
        if time::Instant::now() >= drain_deadline {
            break;
        }
        match time::timeout(Duration::from_millis(100), stream.next()).await {
            Ok(Some(_)) => {}
            _ => break,
        }
    }
    peripheral.unsubscribe(&char).await.unwrap();

    let mut received_after_unsubscribe = false;
    let timeout = time::sleep(Duration::from_secs(2));
    tokio::pin!(timeout);
    loop {
        tokio::select! {
            Some(n) = stream.next() => {
                if n.uuid == gatt_uuids::NOTIFY_CHAR {
                    received_after_unsubscribe = true;
                    break;
                }
            }
            _ = &mut timeout => break,
        }
    }
    assert!(
        !received_after_unsubscribe,
        "Should not receive notifications after unsubscribe"
    );

    // A second unsubscribe must be harmless and leave notifications disabled.
    peripheral.unsubscribe(&char).await.unwrap();
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

/// Covers #326: subscribing to the same characteristic twice must not
/// register a duplicate notification handler, which would deliver every
/// notification more than once.
pub async fn test_resubscribe_does_not_duplicate_notifications() {
    use futures::StreamExt;
    use std::collections::HashSet;
    use std::time::Duration;
    use tokio::time;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::NOTIFY_CHAR);

    tokio::time::timeout(Duration::from_secs(10), peripheral.subscribe(&char))
        .await
        .expect("first subscribe(NOTIFY_CHAR) timed out")
        .expect("first subscribe(NOTIFY_CHAR) should succeed");
    tokio::time::timeout(Duration::from_secs(10), peripheral.subscribe(&char))
        .await
        .expect("second subscribe(NOTIFY_CHAR) timed out")
        .expect("second subscribe(NOTIFY_CHAR) should succeed");

    // Take the notification stream before starting notifications so nothing
    // is missed.
    let mut stream = peripheral.notifications().await.unwrap();
    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_START_NOTIFICATIONS).await;

    let mut received = Vec::new();
    let timeout = time::sleep(Duration::from_secs(4));
    tokio::pin!(timeout);
    loop {
        tokio::select! {
            Some(notification) = stream.next() => {
                if notification.uuid == gatt_uuids::NOTIFY_CHAR {
                    received.push(notification.value);
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

    // NOTIFY_CHAR's payload is a single incrementing counter byte
    // (test-peripheral/zephyr/src/control_service.c periodic_notify_handler:
    // `static uint8_t counter; uint8_t data[] = {counter++};`). If resubscribe
    // duplicated the notification handler, the same counter value would
    // appear more than once in this collection window.
    let mut seen = HashSet::new();
    for value in &received {
        assert_eq!(
            value.len(),
            1,
            "NOTIFY_CHAR payload should be a single counter byte, got {:?}",
            value
        );
        assert!(
            seen.insert(value[0]),
            "counter byte {} repeated -- resubscribe duplicated the notification handler",
            value[0]
        );
    }

    peripheral.disconnect().await.unwrap();
}

/// Covers #471: a subscribe the peripheral refuses must resolve to `Err`, and later subscribes must still work.
pub async fn test_refused_subscribe_returns_error() {
    use futures::StreamExt;
    use std::time::Duration;
    use tokio::time;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let refused_char =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::REFUSED_NOTIFY_CHAR);

    let subscribe_result =
        tokio::time::timeout(Duration::from_secs(10), peripheral.subscribe(&refused_char))
            .await
            .expect("subscribe(REFUSED_NOTIFY_CHAR) timed out");
    assert!(
        subscribe_result.is_err(),
        "expected an error subscribing to REFUSED_NOTIFY_CHAR, got {:?}",
        subscribe_result
    );

    let notify_char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::NOTIFY_CHAR);
    let mut stream = peripheral.notifications().await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), peripheral.subscribe(&notify_char))
        .await
        .expect("subscribe(NOTIFY_CHAR) timed out")
        .expect("subscribe(NOTIFY_CHAR) should succeed after the refused subscribe");
    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_START_NOTIFICATIONS).await;

    let received = time::timeout(Duration::from_secs(5), async {
        loop {
            match stream.next().await {
                Some(notification) if notification.uuid == gatt_uuids::NOTIFY_CHAR => return true,
                Some(_) => continue,
                None => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(
        received,
        "expected a notification on NOTIFY_CHAR after successfully subscribing"
    );

    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_STOP_NOTIFICATIONS).await;
    peripheral.unsubscribe(&notify_char).await.unwrap();
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

// ── Concurrency ─────────────────────────────────────────────────────

pub async fn test_concurrent_connect_and_discover() {
    use futures::join;
    use std::time::Duration;
    use tokio::time::timeout;

    let peripheral = peripheral_finder::find_and_connect().await;
    assert!(peripheral.is_connected().await.unwrap());

    // Start the collector before disconnect() so nothing is lost while it is
    // awaited (see peripheral_finder::spawn_event_collector).
    let mut events = peripheral_finder::spawn_event_collector().await;

    peripheral.disconnect().await.unwrap();
    assert!(!peripheral.is_connected().await.unwrap());

    // On macOS, wait for the peripheral to be rediscovered before attempting
    // to reconnect; other backends keep it connectable across disconnects and
    // this is a no-op (see peripheral_finder::wait_for_rediscovery).
    peripheral_finder::wait_for_rediscovery(&mut events, peripheral.id()).await;

    // Concurrent connect (#488).
    let results = timeout(Duration::from_secs(15), async {
        join!(peripheral.connect(), peripheral.connect())
    })
    .await
    .expect("concurrent connect() calls did not complete within timeout");
    #[cfg(target_os = "linux")]
    {
        // bluez-async's connect_with_timeout issues Device1.Connect() directly
        // with no dedup, so a second concurrent connect() may be rejected
        // with org.bluez.Error.InProgress; require at least one to succeed.
        let (first, second) = results;
        assert!(
            first.is_ok() || second.is_ok(),
            "both concurrent connect() calls failed"
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        let (first, second) = results;
        first.expect("first concurrent connect() failed");
        second.expect("second concurrent connect() failed");
    }
    assert!(
        peripheral.is_connected().await.unwrap(),
        "peripheral should be connected after concurrent connect() calls"
    );

    // Concurrent service discovery (#489 rediscovery).
    let results = timeout(Duration::from_secs(15), async {
        join!(
            peripheral.discover_services(),
            peripheral.discover_services()
        )
    })
    .await
    .expect("concurrent discover_services() calls did not complete within timeout");
    let (first, second) = results;
    first.expect("first concurrent discover_services() failed");
    second.expect("second concurrent discover_services() failed");
    assert!(
        !peripheral.services().is_empty(),
        "services should be populated after concurrent discover_services() calls"
    );

    // Concurrent disconnect.
    let results = timeout(Duration::from_secs(15), async {
        join!(peripheral.disconnect(), peripheral.disconnect())
    })
    .await
    .expect("concurrent disconnect() calls did not complete within timeout");
    let (first, second) = results;
    first.expect("first concurrent disconnect() failed");
    second.expect("second concurrent disconnect() failed");
    assert!(
        !peripheral.is_connected().await.unwrap(),
        "peripheral should be disconnected after concurrent disconnect() calls"
    );
}

pub async fn test_concurrent_operations_same_service() {
    use futures::join;
    use std::time::Duration;
    use tokio::time::timeout;

    let peripheral = peripheral_finder::find_and_connect().await;
    // Subscribing mutates CCC state.
    peripheral_finder::reset_peripheral(&peripheral).await;

    let notify_char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::NOTIFY_CHAR);
    let indicate_char =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::INDICATE_CHAR);

    // Concurrent subscribe to two characteristics in the same service (#481
    // WinRT deadlock).
    let (subscribe_notify, subscribe_indicate) = timeout(Duration::from_secs(15), async {
        join!(
            peripheral.subscribe(&notify_char),
            peripheral.subscribe(&indicate_char)
        )
    })
    .await
    .expect("concurrent subscribe() calls did not complete within timeout");
    subscribe_notify.expect("subscribe(NOTIFY_CHAR) should succeed");
    subscribe_indicate.expect("subscribe(INDICATE_CHAR) should succeed");

    let static_char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::STATIC_READ);
    let counter_char =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::COUNTER_READ);

    // Concurrent read of two characteristics in the same service.
    let (read_static, read_counter) = timeout(Duration::from_secs(15), async {
        join!(
            peripheral.read(&static_char),
            peripheral.read(&counter_char)
        )
    })
    .await
    .expect("concurrent read() calls did not complete within timeout");
    let static_value = read_static.expect("read(STATIC_READ) should succeed");
    read_counter.expect("read(COUNTER_READ) should succeed");
    assert_eq!(
        static_value,
        gatt_uuids::STATIC_READ_VALUE,
        "Static read should return [0x01, 0x02, 0x03, 0x04]"
    );

    peripheral.unsubscribe(&notify_char).await.unwrap();
    peripheral.unsubscribe(&indicate_char).await.unwrap();
    peripheral.disconnect().await.unwrap();
}

pub async fn test_discover_services_during_read() {
    use futures::join;
    use std::time::Duration;
    use tokio::time::timeout;

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    let counter_char =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::COUNTER_READ);

    let (read_result, discover_result) = timeout(Duration::from_secs(15), async {
        join!(
            peripheral.read(&counter_char),
            peripheral.discover_services()
        )
    })
    .await
    .expect("concurrent read()/discover_services() calls did not complete within timeout");
    read_result.expect("read(COUNTER_READ) should succeed during concurrent discovery");
    discover_result.expect("discover_services() should succeed during concurrent read");

    let static_char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::STATIC_READ);
    let static_value = peripheral
        .read(&static_char)
        .await
        .expect("follow-up read(STATIC_READ) should succeed");
    assert_eq!(
        static_value,
        gatt_uuids::STATIC_READ_VALUE,
        "Static read should return [0x01, 0x02, 0x03, 0x04]"
    );

    peripheral.disconnect().await.unwrap();
}

/// Covers #490: a peripheral-triggered disconnect that lands mid-operation
/// must not wedge the command queue or hide the `DeviceDisconnected` event.
///
/// This branch also fixed CoreBluetooth replying to pending operations when a
/// peripheral goes away (`f3de711`, `src/corebluetooth/internal.rs`), so a
/// timeout anywhere in this test is a real regression signal on every
/// platform, not just Android -- it must never be relaxed away.
pub async fn test_operations_across_peripheral_triggered_disconnect() {
    use btleplug::api::CentralEvent;
    use std::time::{Duration, Instant};

    let peripheral = peripheral_finder::find_and_connect().await;
    peripheral_finder::reset_peripheral(&peripheral).await;
    assert!(peripheral.is_connected().await.unwrap());
    let target_id = peripheral.id();

    // Start the collector before sending the first 0x03 so the
    // DeviceDisconnected event can't be lost (see
    // peripheral_finder::spawn_event_collector). Reused for both disconnect
    // cycles; the buffer is drained before cycle 2's 0x03 so no leftover
    // cycle-1 event can satisfy a cycle-2 wait.
    let mut events = peripheral_finder::spawn_event_collector().await;

    let descriptor = super::find_descriptor(
        &peripheral,
        gatt_uuids::DESCRIPTOR_TEST_CHAR,
        gatt_uuids::READ_ONLY_DESCRIPTOR,
    );
    let counter_char =
        peripheral_finder::find_characteristic(&peripheral, gatt_uuids::COUNTER_READ);
    let static_char = peripheral_finder::find_characteristic(&peripheral, gatt_uuids::STATIC_READ);

    // Repeats a single kind of read operation, each bounded by a 5 second
    // timeout, until one returns Err (the firmware disconnects ~500ms after
    // 0x03). No operation may time out -- that would mean the disconnect
    // never surfaced to a pending command. The whole loop is also bounded to
    // ~10s so a regression that swallows the error forever fails instead of
    // hanging. Restricting each cycle to one op kind (instead of alternating)
    // makes it unambiguous which op the disconnect actually caught.
    async fn run_until_disconnect<F, Fut, T, E>(cycle: &str, op_name: &str, mut op: F) -> E
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Debug,
    {
        let loop_deadline = Instant::now() + Duration::from_secs(10);
        let mut success_count = 0u32;
        let mut iteration = 0u32;
        loop {
            iteration += 1;
            if Instant::now() >= loop_deadline {
                panic!(
                    "{cycle}: no {op_name} call returned Err after {success_count} successful \
                     calls within the 10s bound -- the peripheral-triggered disconnect (0x03) \
                     never surfaced to a pending command"
                );
            }

            let result = tokio::time::timeout(Duration::from_secs(5), op())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "{cycle}: {op_name} timed out on iteration {iteration} (after \
                         {success_count} prior successful calls) -- disconnect mid-operation \
                         wedged the command queue"
                    )
                });

            match result {
                Ok(_) => success_count += 1,
                Err(e) => {
                    println!(
                        "{cycle}: {op_name} failed on iteration {iteration} (after \
                         {success_count} prior successful calls) with error: {e:?}"
                    );
                    return e;
                }
            }
        }
    }

    // Cycle 1: only descriptor reads are ever in flight, so the disconnect
    // that ends the loop is unambiguously caught by a descriptor read.
    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_TRIGGER_DISCONNECT).await;
    let _ = run_until_disconnect("cycle 1", "read_descriptor(READ_ONLY_DESCRIPTOR)", || {
        peripheral.read_descriptor(&descriptor)
    })
    .await;

    peripheral_finder::wait_for_event(
        &mut events,
        Duration::from_secs(10),
        "DeviceDisconnected(test peripheral) [cycle 1]",
        |event| matches!(event, CentralEvent::DeviceDisconnected(id) if *id == target_id),
    )
    .await;
    assert!(
        !peripheral.is_connected().await.unwrap(),
        "peripheral should be disconnected after cycle 1"
    );

    // One more read after disconnection must return Err promptly, not hang --
    // proof the queue isn't wedged and the disconnect is visible to new ops.
    // Checked once here; cycle 2 below re-proves the queue survives a second
    // disconnect via its own reconnect + STATIC_READ at the end.
    let post_disconnect_result = tokio::time::timeout(
        Duration::from_secs(2),
        peripheral.read(&counter_char),
    )
    .await
    .expect("read(COUNTER_READ) after disconnection timed out -- command queue is wedged (#490)");
    // Peripheral.java clears `connected` under its lock (~L551) before
    // `adapter.onConnectionStateChanged` (~L564) emits `DeviceDisconnected`, so
    // `read()`'s `!this.connected` guard (~L198) throws `NotConnectedException`,
    // mapped by `get_poll_result` (`src/droidplug/peripheral.rs` ~L92) to `Error::NotConnected`.
    #[cfg(target_os = "android")]
    assert!(
        matches!(post_disconnect_result, Err(btleplug::Error::NotConnected)),
        "expected Error::NotConnected reading after disconnection on Android, got {:?}",
        post_disconnect_result
    );
    #[cfg(not(target_os = "android"))]
    assert!(
        post_disconnect_result.is_err(),
        "expected an error reading after disconnection, got {:?}",
        post_disconnect_result
    );

    // Reconnect, rediscover, and reset before cycle 2.
    peripheral_finder::wait_for_rediscovery(&mut events, target_id.clone()).await;

    tokio::time::timeout(Duration::from_secs(10), peripheral.connect())
        .await
        .expect("connect() after cycle 1 disconnect timed out")
        .expect("connect() after cycle 1 disconnect failed");
    assert!(peripheral.is_connected().await.unwrap());

    tokio::time::timeout(Duration::from_secs(10), peripheral.discover_services())
        .await
        .expect("discover_services() after cycle 1 timed out")
        .expect("discover_services() after cycle 1 failed");

    peripheral_finder::reset_peripheral(&peripheral).await;

    // Cycle 2: only characteristic reads are ever in flight, so the
    // disconnect that ends the loop is unambiguously caught by a
    // characteristic read.
    while events.try_recv().is_ok() {}
    peripheral_finder::send_control_command(&peripheral, gatt_uuids::CMD_TRIGGER_DISCONNECT).await;
    let _ = run_until_disconnect("cycle 2", "read(COUNTER_READ)", || {
        peripheral.read(&counter_char)
    })
    .await;

    peripheral_finder::wait_for_event(
        &mut events,
        Duration::from_secs(10),
        "DeviceDisconnected(test peripheral) [cycle 2]",
        |event| matches!(event, CentralEvent::DeviceDisconnected(id) if *id == target_id),
    )
    .await;
    assert!(
        !peripheral.is_connected().await.unwrap(),
        "peripheral should be disconnected after cycle 2"
    );

    // Reconnect, rediscover, and read STATIC_READ to prove the command queue
    // is not wedged.
    peripheral_finder::wait_for_rediscovery(&mut events, target_id).await;

    tokio::time::timeout(Duration::from_secs(10), peripheral.connect())
        .await
        .expect("connect() after cycle 2 disconnect timed out")
        .expect("connect() after cycle 2 disconnect failed");
    assert!(peripheral.is_connected().await.unwrap());

    tokio::time::timeout(Duration::from_secs(10), peripheral.discover_services())
        .await
        .expect("discover_services() after cycle 2 timed out")
        .expect("discover_services() after cycle 2 failed");

    let value = tokio::time::timeout(Duration::from_secs(10), peripheral.read(&static_char))
        .await
        .expect("read(STATIC_READ) timed out")
        .expect("read(STATIC_READ) failed");
    assert_eq!(
        value,
        gatt_uuids::STATIC_READ_VALUE,
        "Static read should return [0x01, 0x02, 0x03, 0x04]"
    );

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
