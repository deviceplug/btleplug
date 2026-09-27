mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_gatt_error_status_is_reported() {
    common::test_cases::test_gatt_error_status_is_reported().await;
}
