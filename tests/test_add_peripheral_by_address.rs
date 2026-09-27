mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_add_peripheral_by_address() {
    common::test_cases::test_add_peripheral_by_address().await;
}
