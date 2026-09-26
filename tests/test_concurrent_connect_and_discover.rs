mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_concurrent_connect_and_discover() {
    common::test_cases::test_concurrent_connect_and_discover().await;
}
