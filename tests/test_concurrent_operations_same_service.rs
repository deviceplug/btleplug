mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_concurrent_operations_same_service() {
    common::test_cases::test_concurrent_operations_same_service().await;
}
