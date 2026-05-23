//! Android integration test native library.
//!
//! This cdylib exposes each integration test as a JNI function callable from
//! Kotlin instrumentation tests. It reuses the shared test logic from
//! `tests/common/test_cases.rs` via `#[path]` includes.

#[path = "../../../common/gatt_uuids.rs"]
#[allow(dead_code)]
mod gatt_uuids;

#[path = "../../../common/peripheral_finder.rs"]
mod peripheral_finder;

#[path = "../../../common/test_cases.rs"]
mod test_cases;

// The `common::find_descriptor` function is used by descriptor tests in test_cases.
// It references `super::find_descriptor`, so we provide it at crate root.
pub fn find_descriptor(
    peripheral: &btleplug::platform::Peripheral,
    char_uuid: uuid::Uuid,
    descriptor_uuid: uuid::Uuid,
) -> btleplug::api::Descriptor {
    use btleplug::api::Peripheral as _;
    let services = peripheral.services();
    for service in &services {
        for char in &service.characteristics {
            if char.uuid == char_uuid {
                for desc in &char.descriptors {
                    if desc.uuid == descriptor_uuid {
                        return desc.clone();
                    }
                }
            }
        }
    }
    panic!(
        "descriptor {} not found on characteristic {}",
        descriptor_uuid, char_uuid
    );
}

use jni::objects::JClass;
use jni::{Env, EnvUnowned, jni_str};
use jni::errors::ThrowRuntimeExAndDefault;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

/// Process-global tokio runtime shared across all test invocations.
fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        Runtime::new().expect("failed to create tokio runtime")
    })
}

/// Per-test timeout on the Rust side. Must be shorter than the JUnit Timeout rule (60s)
/// so that the Rust side reports the failure before JUnit kills the thread.
const TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(55);

/// Run a test function on the global runtime, converting panics to JNI exceptions.
fn run_test(env: &mut Env, test_name: &str, f: impl std::future::Future<Output = ()>) {
    log::info!("[START] {}", test_name);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(async {
            tokio::time::timeout(TEST_TIMEOUT, f)
                .await
                .expect("test timed out (Rust-side 55s limit)")
        });
    }));
    match &result {
        Ok(()) => log::info!("[PASS] {}", test_name),
        Err(panic) => {
            let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "test panicked".to_string()
            };
            log::error!("[FAIL] {}: {}", test_name, msg);
            let jni_msg = jni::strings::JNIString::from(msg.as_str());
            env.throw_new(jni_str!("java/lang/RuntimeException"), &jni_msg).ok();
        }
    }
}

/// Initialize btleplug's Android/JNI layer. Must be called once before any tests.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_nonpolynomial_btleplug_test_NativeTests_initBtleplug(
    mut env: EnvUnowned,
    _class: JClass,
) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("btleplug-test"),
    );
    env.with_env(|env| btleplug::platform::init(env))
        .resolve::<ThrowRuntimeExAndDefault>();
}

// ── Test JNI exports ────────────────────────────────────────────────
//
// Each function follows the JNI naming convention:
//   Java_com_nonpolynomial_btleplug_test_NativeTests_<methodName>

macro_rules! jni_test {
    ($jni_name:ident, $test_fn:path) => {
        #[unsafe(no_mangle)]
        pub extern "system" fn $jni_name(mut env: EnvUnowned, _class: JClass) {
            env.with_env(|env| {
                run_test(env, stringify!($test_fn), $test_fn());
                Ok::<_, jni::errors::Error>(())
            })
            .resolve::<ThrowRuntimeExAndDefault>();
        }
    };
}

jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testDiscoverPeripheralByName,
    test_cases::test_discover_peripheral_by_name
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testDiscoverServices,
    test_cases::test_discover_services
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testDiscoverCharacteristics,
    test_cases::test_discover_characteristics
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testScanFilterByServiceUuid,
    test_cases::test_scan_filter_by_service_uuid
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testAdvertisementManufacturerData,
    test_cases::test_advertisement_manufacturer_data
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testAdvertisementServices,
    test_cases::test_advertisement_services
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testConnectAndDisconnect,
    test_cases::test_connect_and_disconnect
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testReconnectAfterDisconnect,
    test_cases::test_reconnect_after_disconnect
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testPeripheralTriggeredDisconnect,
    test_cases::test_peripheral_triggered_disconnect
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testReadStaticValue,
    test_cases::test_read_static_value
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testReadCounterIncrements,
    test_cases::test_read_counter_increments
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testWriteWithResponse,
    test_cases::test_write_with_response
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testWriteWithoutResponse,
    test_cases::test_write_without_response
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testWriteWithoutResponseBurst,
    test_cases::test_write_without_response_burst
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testReadWriteRoundtrip,
    test_cases::test_read_write_roundtrip
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testLongValueReadWrite,
    test_cases::test_long_value_read_write
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testCharacteristicProperties,
    test_cases::test_characteristic_properties
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testSubscribeAndReceiveNotifications,
    test_cases::test_subscribe_and_receive_notifications
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testSubscribeAndReceiveIndications,
    test_cases::test_subscribe_and_receive_indications
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testUnsubscribeStopsNotifications,
    test_cases::test_unsubscribe_stops_notifications
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testConfigurableNotificationPayload,
    test_cases::test_configurable_notification_payload
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testReadOnlyDescriptor,
    test_cases::test_read_only_descriptor
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testReadWriteDescriptorRoundtrip,
    test_cases::test_read_write_descriptor_roundtrip
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testDescriptorDiscovery,
    test_cases::test_descriptor_discovery
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testMtuAfterConnection,
    test_cases::test_mtu_after_connection
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testReadRssi,
    test_cases::test_read_rssi
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testPropertiesContainPeripheralInfo,
    test_cases::test_properties_contain_peripheral_info
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testConnectionParameters,
    test_cases::test_connection_parameters
);
jni_test!(
    Java_com_nonpolynomial_btleplug_test_NativeTests_testRequestConnectionParameters,
    test_cases::test_request_connection_parameters
);
