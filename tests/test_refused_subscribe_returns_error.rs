mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_refused_subscribe_returns_error() {
    common::test_cases::test_refused_subscribe_returns_error().await;
}
