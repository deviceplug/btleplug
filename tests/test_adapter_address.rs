mod common;

#[tokio::test]
#[ignore = "requires a local Bluetooth adapter"]
async fn test_adapter_address() {
    common::test_cases::test_adapter_address().await;
}
