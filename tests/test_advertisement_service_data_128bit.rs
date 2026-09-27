mod common;

#[tokio::test]
#[ignore = "requires BLE test peripheral"]
async fn test_advertisement_service_data_128bit() {
    common::test_cases::test_advertisement_service_data_128bit().await;
}
