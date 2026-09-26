mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_discover_services_during_read() {
    common::test_cases::test_discover_services_during_read().await;
}
