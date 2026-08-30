mod common;

#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_clear_peripherals_rediscovers_device() {
    common::test_cases::test_clear_peripherals_rediscovers_device().await;
}
