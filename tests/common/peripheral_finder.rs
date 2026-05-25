//! Helper to discover and connect to the btleplug test peripheral.

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;
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
