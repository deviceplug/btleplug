//! Helper to discover and connect to the btleplug test peripheral.

use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::time;

use super::gatt_uuids;

/// Default scan timeout — how long to wait for the test peripheral to appear.
const DEFAULT_SCAN_TIMEOUT: Duration = Duration::from_secs(15);

/// Tracks whether a background scan is running. Android throttles apps to
/// 5 BLE scan starts per 30 seconds — exceeding this causes scans to silently
/// fail. We keep a single scan running across tests to stay within budget.
static SCAN_RUNNING: AtomicBool = AtomicBool::new(false);

/// Process-global adapter. Reusing a single CBCentralManager on macOS is critical —
/// creating a second one in the same process causes CoreBluetooth to stop reporting
/// peripherals that were discovered by the first.
static ADAPTER: OnceCell<Adapter> = OnceCell::const_new();

pub async fn get_adapter() -> &'static Adapter {
    ADAPTER
        .get_or_init(|| async {
            // Create the adapter on a dedicated thread with its own tokio runtime.
            // This ensures the event-processing task spawned by Adapter::new()
            // survives across #[tokio::test] runtime boundaries (each test gets
            // its own runtime, which shuts down after the test completes).
            let (tx, rx) = tokio::sync::oneshot::channel();
            std::thread::Builder::new()
                .name("btleplug-test-adapter".into())
                .spawn(move || {
                    log::info!("adapter thread: starting");
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .expect("failed to create adapter runtime");
                        rt.block_on(async {
                            log::info!("adapter thread: creating manager");
                            let manager =
                                Manager::new().await.expect("failed to create BLE manager");
                            log::info!("adapter thread: getting adapters");
                            let adapters =
                                manager.adapters().await.expect("failed to get adapters");
                            log::info!("adapter thread: got {} adapters", adapters.len());
                            std::mem::forget(manager);
                            let adapter =
                                adapters.into_iter().next().expect("no BLE adapters found");
                            log::info!("adapter thread: sending adapter");
                            tx.send(adapter).ok();
                            log::info!("adapter thread: blocking forever");
                            std::future::pending::<()>().await;
                        });
                    }));
                    if let Err(panic) = result {
                        let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "unknown panic".to_string()
                        };
                        log::error!("adapter thread PANICKED: {}", msg);
                    }
                })
                .expect("failed to spawn adapter thread");
            rx.await
                .expect("failed to receive adapter from background thread")
        })
        .await
}

/// Best-effort cleanup: disconnect any connected peripherals.
/// This prevents state leakage between tests when running in a shared process (Android).
/// The background scan is intentionally kept running to avoid Android scan throttling.
async fn ensure_clean_state(adapter: &Adapter) {
    // Use timeouts on all operations — a hung disconnect from a prior test
    // must not block subsequent tests forever.
    if let Ok(Ok(peripherals)) =
        tokio::time::timeout(Duration::from_secs(2), adapter.peripherals()).await
    {
        for p in peripherals {
            if let Ok(Ok(true)) =
                tokio::time::timeout(Duration::from_secs(1), p.is_connected()).await
            {
                let _ = tokio::time::timeout(Duration::from_secs(5), p.disconnect()).await;
            }
        }
    }
    // Give the BLE stack time to settle after disconnection.
    tokio::time::sleep(Duration::from_secs(2)).await;
}

/// Discover the test peripheral by name, connect to it, and discover its services.
///
/// Returns the connected `Peripheral` with services already discovered.
///
/// # Panics
/// Panics if the peripheral is not found within the timeout, or if connection/service
/// discovery fails.
pub async fn find_and_connect() -> Peripheral {
    let peripheral_name = std::env::var("BTLEPLUG_TEST_PERIPHERAL")
        .unwrap_or_else(|_| gatt_uuids::TEST_PERIPHERAL_NAME.to_string());

    let adapter = get_adapter().await;

    // Clean up any lingering state from a prior test (disconnect peripherals).
    ensure_clean_state(adapter).await;

    // Start a background scan if one isn't already running.
    // We keep the scan running across tests to avoid Android's BLE scan
    // throttling (5 starts per 30s — exceeding this silently drops results).
    if !SCAN_RUNNING.load(Ordering::Relaxed) {
        adapter
            .start_scan(ScanFilter::default())
            .await
            .expect("failed to start scan");
        SCAN_RUNNING.store(true, Ordering::Relaxed);
    }

    let peripheral = tokio::time::timeout(DEFAULT_SCAN_TIMEOUT, async {
        let start = tokio::time::Instant::now();
        let mut scan_restarted = false;
        loop {
            let peripherals = adapter
                .peripherals()
                .await
                .expect("failed to list peripherals");
            for p in peripherals {
                if let Ok(Some(props)) = p.properties().await {
                    if props.local_name.as_deref() == Some(&peripheral_name) {
                        return p;
                    }
                }
            }
            // If we haven't found anything after 5s, another test may have
            // stopped the scan (e.g. scan-filter tests). Restart it once.
            if !scan_restarted && start.elapsed() >= Duration::from_secs(5) {
                let _ = adapter.start_scan(ScanFilter::default()).await;
                SCAN_RUNNING.store(true, Ordering::Relaxed);
                scan_restarted = true;
            }
            time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "timed out after {:?} waiting for peripheral '{}'",
            DEFAULT_SCAN_TIMEOUT, peripheral_name
        )
    });

    peripheral
        .connect_with_timeout(Duration::from_secs(10))
        .await
        .expect("failed to connect to test peripheral");

    peripheral
        .discover_services_with_timeout(Duration::from_secs(10))
        .await
        .expect("failed to discover services");
    peripheral
}

/// Send a control command to the test peripheral's Control Point characteristic.
pub async fn send_control_command(peripheral: &Peripheral, opcode: u8) {
    let chars = peripheral.characteristics();
    let control_point = chars
        .iter()
        .find(|c| c.uuid == gatt_uuids::CONTROL_POINT)
        .expect("Control Point characteristic not found");

    peripheral
        .write(control_point, &[opcode], WriteType::WithResponse)
        .await
        .expect("failed to write control command");
}

/// Reset the test peripheral to its default state.
pub async fn reset_peripheral(peripheral: &Peripheral) {
    send_control_command(peripheral, gatt_uuids::CMD_RESET_STATE).await;
    // Brief pause to let the peripheral process the reset
    time::sleep(Duration::from_millis(100)).await;
}

/// Find a characteristic by UUID from the peripheral's discovered characteristics.
pub fn find_characteristic(
    peripheral: &Peripheral,
    uuid: uuid::Uuid,
) -> btleplug::api::Characteristic {
    peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == uuid)
        .unwrap_or_else(|| panic!("characteristic {} not found", uuid))
}

/// Subscribe to `adapter.events()` and forward every event into an unbounded
/// channel.
///
/// The central event broadcast channel (`src/common/adapter_manager.rs`) has
/// capacity 16 and `event_stream()` silently drops events for a receiver that
/// falls behind (`filter_map(|x| x.ok())`). During an active scan CoreBluetooth
/// emits `DeviceUpdated` for every nearby advertiser, so a receiver that isn't
/// polled continuously can lose events (in particular `DeviceConnected`)
/// between the moment it subscribes and the moment a test gets around to
/// reading it. Forwarding into an unbounded channel from a task that starts
/// draining immediately avoids that loss. The subscription (`events().await`)
/// happens before the forwarding task is spawned, so no event between the
/// caller's request and the task starting is missed.
pub async fn spawn_event_collector() -> UnboundedReceiver<CentralEvent> {
    use futures::StreamExt;

    let adapter = get_adapter().await;
    let mut events = adapter
        .events()
        .await
        .expect("failed to subscribe to adapter events");
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tx.closed() => break,
                ev = events.next() => match ev {
                    Some(e) => {
                        if tx.send(e).is_err() {
                            break;
                        }
                    }
                    None => break,
                },
            }
        }
    });
    rx
}

/// Wait for an event matching `matches` to arrive on `rx`, ignoring events
/// that don't match (e.g. events for other peripherals' ids). Panics with a
/// clear message identifying `what` was being waited for if `timeout` elapses
/// first, or if the collector task has exited (the underlying event stream
/// ended).
pub async fn wait_for_event<F>(
    rx: &mut UnboundedReceiver<CentralEvent>,
    timeout: Duration,
    what: &str,
    mut matches: F,
) -> CentralEvent
where
    F: FnMut(&CentralEvent) -> bool,
{
    tokio::time::timeout(timeout, async {
        loop {
            match rx.recv().await {
                Some(event) if matches(&event) => return event,
                Some(_) => continue,
                None => panic!("event collector channel closed while waiting for {}", what),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out after {:?} waiting for {}", timeout, what))
}

/// Wait for the peripheral identified by `target_id` to be genuinely
/// connected, discarding a stale `DeviceConnected(target_id)` that does not
/// reflect the current connection.
///
/// On BlueZ, `events()` synthesises a `DeviceConnected` for every
/// already-connected device the moment the stream is opened
/// (`src/bluez/adapter.rs`), and BlueZ keeps LE links alive after a process
/// exits. So after a prior test panicked mid-connection, a fresh collector
/// can see a stale `DeviceConnected(id)`, then a `DeviceDisconnected(id)`
/// from this test's own `ensure_clean_state()`, then the real
/// `DeviceConnected(id)` -- three events for what looks like one connection.
///
/// This waits for `DeviceConnected(target_id)`, then drains everything
/// already buffered for `target_id` (after yielding once so a concurrently
/// running collector task has a chance to enqueue anything it already
/// received), keeping only the latest connection-state event seen. If that
/// latest event is a `DeviceDisconnected(target_id)`, the `DeviceConnected`
/// was stale, so it goes back to waiting for another one. It returns only
/// once the buffer is empty and the latest event for `target_id` is a
/// `DeviceConnected`.
pub async fn wait_for_connected(
    rx: &mut UnboundedReceiver<CentralEvent>,
    target_id: &PeripheralId,
    timeout: Duration,
) {
    let what = "DeviceConnected(test peripheral)";
    tokio::time::timeout(timeout, async {
        loop {
            // (1) Wait for a DeviceConnected(target_id).
            loop {
                match rx.recv().await {
                    Some(CentralEvent::DeviceConnected(id)) if id == *target_id => break,
                    Some(_) => continue,
                    None => panic!("event collector channel closed while waiting for {}", what),
                }
            }

            // Give the collector task a chance to enqueue anything it
            // already received before we drain what's buffered.
            tokio::task::yield_now().await;

            // (2) Drain everything currently buffered, tracking the latest
            // connection-state event seen for target_id.
            let mut last_was_connected = true;
            loop {
                match rx.try_recv() {
                    Ok(CentralEvent::DeviceConnected(id)) if id == *target_id => {
                        last_was_connected = true;
                    }
                    Ok(CentralEvent::DeviceDisconnected(id)) if id == *target_id => {
                        last_was_connected = false;
                    }
                    Ok(_) => continue,
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        panic!("event collector channel closed while waiting for {}", what)
                    }
                }
            }

            // (3) A DeviceDisconnected(target_id) after the DeviceConnected
            // we found means it was stale; go back to waiting for another.
            if last_was_connected {
                return;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out after {:?} waiting for {}", timeout, what))
}

/// Wait, if needed, for a disconnected peripheral to become connectable again.
///
/// CoreBluetooth removes a peripheral from its internal map on disconnect and
/// only re-inserts it once the background scan rediscovers it
/// (`src/corebluetooth/internal.rs` `on_peripheral_disconnect`; intentional,
/// issue #57), and `AdapterManager::emit` removes it from the public map on
/// `DeviceDisconnected`. `connect()` fails fast with "Peripheral no longer
/// available" until then. Other backends keep the peripheral connectable
/// across a disconnect, so this is a no-op there.
///
/// `rx` must be a collector started before the disconnect. The caller may
/// already have consumed the `DeviceDisconnected` event itself before calling
/// this: once disconnected, `AdapterManager` has dropped the id from its map,
/// so rediscovery always emits `DeviceDiscovered(id)` first regardless of
/// whether `DeviceDisconnected` was observed by this call.
pub async fn wait_for_rediscovery(
    rx: &mut UnboundedReceiver<CentralEvent>,
    target_id: PeripheralId,
) {
    #[cfg(target_vendor = "apple")]
    {
        // A `DeviceDiscovered(target_id)` is always fresh, since CoreBluetooth
        // only emits it for an id currently absent from AdapterManager, and
        // AdapterManager removes the id on `DeviceDisconnected`. A
        // `DeviceUpdated(target_id)` is only trustworthy once we've also seen
        // the id actually leave and re-enter the map (`DeviceDisconnected` or
        // `DeviceDiscovered`), since one queued before our disconnect would
        // otherwise be indistinguishable from a post-rediscovery update.
        let mut seen_removed_or_discovered = false;
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                match rx.recv().await {
                    Some(CentralEvent::DeviceDiscovered(id)) if id == target_id => break,
                    Some(CentralEvent::DeviceDisconnected(id)) if id == target_id => {
                        seen_removed_or_discovered = true;
                    }
                    Some(CentralEvent::DeviceUpdated(id))
                        if id == target_id && seen_removed_or_discovered =>
                    {
                        break;
                    }
                    Some(_) => continue,
                    None => panic!("event collector channel closed while waiting for rediscovery"),
                }
            }
        })
        .await
        .expect("timed out waiting for peripheral rediscovery after disconnect");
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        // Other backends keep the peripheral connectable across a disconnect;
        // nothing to wait for.
        let _ = (rx, target_id);
    }
}
