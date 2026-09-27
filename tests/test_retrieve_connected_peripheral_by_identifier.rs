mod common;

#[cfg(not(target_os = "android"))]
#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_retrieve_connected_peripheral_by_identifier() {
    common::test_cases::test_retrieve_connected_peripheral_by_identifier().await;
}
