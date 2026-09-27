mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_operations_across_peripheral_triggered_disconnect() {
    common::test_cases::test_operations_across_peripheral_triggered_disconnect().await;
}
