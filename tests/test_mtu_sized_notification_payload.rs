mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_mtu_sized_notification_payload() {
    common::test_cases::test_mtu_sized_notification_payload().await;
}
