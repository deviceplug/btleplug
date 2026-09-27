mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_discovery_with_included_service() {
    common::test_cases::test_discovery_with_included_service().await;
}
