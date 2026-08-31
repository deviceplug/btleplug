#[test]
fn readme_distinguishes_adapter_and_peripheral_addresses() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("Retrieve local adapter address"));
    assert!(readme.contains(
        "| Retrieve local adapter address         | X       |             | X     |         |"
    ));
    assert!(readme.contains("Discover MAC address"));
    assert!(readme.contains("PeripheralId"));
    assert!(readme.contains("distinct from a discovered peripheral address"));
}

#[test]
fn central_adapter_address_default_is_source_compatible() {
    fn assert_default<T: btleplug::api::Central>() {}
    assert_default::<btleplug::platform::Adapter>();
}

#[test]
fn properties_and_event_example_document_current_async_contract() {
    let api = include_str!("../src/api/mod.rs");
    assert!(api.contains("`Ok(Some(_))` contains a snapshot"));
    assert!(api.contains("`Ok(None)` means that the backend has no properties snapshot"));

    let example = include_str!("../examples/event_driven_discovery.rs");
    assert!(example.contains("Process events asynchronously"));
    assert!(example.contains("Tokio task"));
    assert!(!example.contains("event receiver blocks"));
    assert!(!example.contains("does not yet use async channels"));
}
