# Zephyr Test Peripheral - Debugging Findings

Date: 2026-03-01

## Build Issues (FIXED)

1. **`BT_LE_ADV_CONN` removed in Zephyr v4.3** — replaced with `BT_LE_ADV_CONN_FAST_1` (30-60ms intervals). Fix in `src/main.c`.

2. **Missing `intelhex` Python package** — needed by `nrfjprog` flash runner. Fix: `uv pip install intelhex` (now installed via requirements-base.txt).

3. **Scan response too large** — adding `BT_DATA_UUID128_ALL` (18 bytes) + `BT_DATA_SVC_DATA128` (19 bytes) exceeds the 31-byte BLE legacy advertising limit. Fix: removed `svc_data`, kept only UUID list.

## Hardware Setup

- Board: nRF52840 DK (S/N 683223459)
- J-Link mass storage disabled via `JLinkExe` → `MSDDisable` (persistent, fixes macOS "Disk Not Ejected Properly" spam)
- Flash command: `west flash --runner nrfjprog` (from `test-peripheral/zephyr/`, with `test-peripheral/.venv` activated)
- Build command: `west build -b nrf52840dk/nrf52840 --pristine` (from `test-peripheral/zephyr/`)

**Pinned workspace note:** the firmware now builds against a Zephyr `v4.4.2` tree pinned by `test-peripheral/zephyr/west.yml`, fetched into `test-peripheral/deps/` via `west init -l zephyr && west update` (from `test-peripheral/`), with a `uv`-managed `.venv` providing `west` and Zephyr's Python build requirements. See `test-peripheral/README.md` for the full one-time setup (including the Zephyr SDK 1.0.1 install). The commands above assume that workspace already exists and `.venv` is activated.

## UART Serial Issues (UNRESOLVED)

- UART console at 115200 baud produces **garbled output** on this board
- Tried: `CONFIG_LOG_MODE_IMMEDIATE=y`, `CONFIG_UART_INTERRUPT_DRIVEN=y` — still garbled
- The DTS confirms `uart0` at `0x1c200` (115200 baud), pins are correct
- **Root cause likely**: UARTE (DMA-based) + immediate logging causes buffer corruption when log messages are emitted from ISR context
- **Workaround**: Use Segger RTT instead of UART for logging:
  ```
  CONFIG_USE_SEGGER_RTT=y
  CONFIG_LOG_BACKEND_RTT=y
  CONFIG_LOG_BACKEND_UART=n
  CONFIG_RTT_CONSOLE=y
  ```
- RTT capture: `JLinkRTTLogger -Device NRF52840_XXAA -If SWD -Speed 4000 -RTTChannel 0 /tmp/rtt_log.txt`
- **Note**: JLinkRTTLogger can't find RTT Control Block after `nrfjprog --reset` — must connect while board is already running, not immediately after reset.

## Advertising Restart After Disconnect (FIXED)

**Problem**: After a BLE connection + disconnect cycle, the peripheral never re-advertised. Tests that ran after a connecting test would time out scanning for the peripheral.

**Root cause**: `bt_le_adv_start()` was called directly from the `disconnected` callback, which runs in the BLE RX thread context. The BLE controller hasn't fully completed the disconnect procedure at that point, so `bt_le_adv_start()` silently fails.

**Fix**: Defer advertising restart to the system workqueue with a 100ms delay:

```c
static void restart_adv_work_handler(struct k_work *work)
{
    bt_le_adv_stop();
    int err = bt_le_adv_start(BT_LE_ADV_CONN_FAST_1, ad, ARRAY_SIZE(ad),
                               sd, ARRAY_SIZE(sd));
    if (err) {
        LOG_ERR("Advertising restart failed (err %d)", err);
    }
}

static K_WORK_DELAYABLE_DEFINE(restart_adv_work, restart_adv_work_handler);

// In disconnected callback:
k_work_reschedule(&restart_adv_work, K_MSEC(100));
```

This was verified: cross-process tests pass after connect+disconnect with this fix.

## Test Infrastructure Issues (PARTIALLY FIXED)

### CoreBluetooth in-process caching

**Problem**: When multiple tests run in the same process, creating a new `Manager` (→ new `CBCentralManager`) causes CoreBluetooth to stop reporting peripherals discovered by a previous Manager instance.

**Partial fix**: Share a single `Manager` and `Adapter` across all tests using `tokio::sync::OnceCell`:

```rust
static ADAPTER: OnceCell<Adapter> = OnceCell::const_new();
```

The Manager is leaked (`std::mem::forget`) so the underlying `CBCentralManager` stays alive for the process lifetime.

**Status**: This fixes some in-process test failures but not all. Event-based tests (`ServicesAdvertisement`, `ManufacturerDataAdvertisement`) still don't receive events for already-cached peripherals when run after other tests in the same process.

### Connect hangs

**Problem**: `peripheral.connect().await` can hang indefinitely if the peripheral is not advertising or CoreBluetooth is in a bad state.

**Fix**: Added 10-second timeouts around `connect()` and `discover_services()` in `find_and_connect()`.

## Remaining Work

1. **In-process test isolation**: Tests that use `adapter.events()` (advertisement event tests) don't work reliably after other tests in the same process. Options:
   - Run each test as a separate process invocation
   - Accept that advertisement event tests must run first (before any connecting tests)
   - Investigate if `adapter.stop_scan()` + delay + `adapter.start_scan()` forces CoreBluetooth to re-emit events

2. **Connection test suite**: `test_connection.rs` tests hang even after board reset. Need to investigate whether `find_and_connect()` or the test body itself hangs. The connect timeout should now prevent indefinite hangs.

3. **GPIO/LED indicators**: Didn't work — LEDs never turned on despite correct DT aliases and CONFIG_GPIO=y. Removed the GPIO code. Low priority to debug.

4. **Run remaining test suites**: `test_read_write`, `test_notifications`, `test_descriptors`, `test_device_info` have not been tested yet.

## Current prj.conf

```
CONFIG_BT=y
CONFIG_BT_PERIPHERAL=y
CONFIG_BT_DEVICE_NAME="btleplug-test"
CONFIG_BT_DEVICE_NAME_DYNAMIC=n
CONFIG_BT_ATT_PREPARE_COUNT=5
CONFIG_BT_BUF_ACL_RX_SIZE=251
CONFIG_BT_BUF_ACL_TX_SIZE=251
CONFIG_BT_L2CAP_TX_MTU=247
CONFIG_LOG=y
CONFIG_BT_LOG_LEVEL_INF=y
CONFIG_USE_SEGGER_RTT=y
CONFIG_LOG_BACKEND_RTT=y
CONFIG_LOG_BACKEND_UART=n
CONFIG_CONSOLE=y
CONFIG_RTT_CONSOLE=y
CONFIG_SYSTEM_WORKQUEUE_STACK_SIZE=2048
CONFIG_HEAP_MEM_POOL_SIZE=4096
```

## Files Modified

- `test-peripheral/zephyr/src/main.c` — `BT_LE_ADV_CONN` → `BT_LE_ADV_CONN_FAST_1`, added UUID128_ALL to scan response, deferred adv restart on disconnect
- `test-peripheral/zephyr/prj.conf` — RTT logging config
- `tests/common/peripheral_finder.rs` — shared adapter singleton, connect timeouts, ScanFilter::default()
- `tests/test_discovery.rs` — uses shared adapter from peripheral_finder
