fn main() {
    build::build()
}

#[cfg(not(target_os = "linux"))]
mod build {
    pub fn build() {}
}

#[cfg(target_os = "linux")]
mod build {
    use std::path::{Path, PathBuf};
    pub fn build() {
        // Only rebuild if the script, or one of the XML files is changed.
        println!("cargo:rerun-if-changed=build.rs");
        let options = dbus_codegen::GenOpts {
            methodtype: None,
            genericvariant: true,
            ..dbus_codegen::GenOpts::default()
        };

        let base_path = Path::new("src/bluez/bluez_dbus/");

        generate_dbus_interfaces(
            base_path.join("bluez-dbus-introspect-manager.xml"),
            base_path.join("manager.rs"),
            &options,
        )
        .unwrap();
        generate_dbus_interfaces(
            base_path.join("bluez-dbus-introspect-adapter.xml"),
            base_path.join("adapter.rs"),
            &options,
        )
        .unwrap();
        generate_dbus_interfaces(
            base_path.join("bluez-dbus-introspect-device.xml"),
            base_path.join("device.rs"),
            &options,
        )
        .unwrap();
        generate_dbus_interfaces(
            base_path.join("bluez-dbus-introspect-gatt-service.xml"),
            base_path.join("gatt_service.rs"),
            &options,
        )
        .unwrap();
        generate_dbus_interfaces(
            base_path.join("bluez-dbus-introspect-gatt-characteristic.xml"),
            base_path.join("gatt_characteristic.rs"),
            &options,
        )
        .unwrap();
        generate_dbus_interfaces(
            base_path.join("bluez-dbus-introspect-gatt-descriptor.xml"),
            base_path.join("gatt_descriptor.rs"),
            &options,
        )
        .unwrap();
    }

    fn generate_dbus_interfaces(
        input_file: PathBuf,
        output_file: PathBuf,
        options: &dbus_codegen::GenOpts,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Only rerun this build script if the input file has changed
        println!("cargo:rerun-if-changed={}", input_file.display());

        let contents = std::fs::read_to_string(&input_file)?;

        let output = dbus_codegen::generate(
            &contents,
            &dbus_codegen::GenOpts {
                command_line: format!(
                    "--generic-variant --methodtype None --file {} --output {}",
                    input_file.display(),
                    output_file.display()
                ),
                ..options.clone()
            },
        )?;

        std::fs::write(output_file, output)?;

        Ok(())
    }
}
