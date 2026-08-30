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
