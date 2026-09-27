mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_reconnect_after_peripheral_triggered_disconnect() {
    common::test_cases::test_reconnect_after_peripheral_triggered_disconnect().await;
}
