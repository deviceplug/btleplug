mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_mtu_after_service_discovery() {
    common::test_cases::test_mtu_after_service_discovery().await;
}
