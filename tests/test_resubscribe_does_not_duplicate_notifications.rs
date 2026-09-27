mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_resubscribe_does_not_duplicate_notifications() {
    common::test_cases::test_resubscribe_does_not_duplicate_notifications().await;
}
