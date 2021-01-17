fn main() {
    build::build()
}

mod build {
    use std::{env, path::{Path, PathBuf}};
    pub fn build() {
        // Only rebuild if the script, or one of the XML files is changed.
        println!("cargo:rerun-if-changed=build.rs");
        let options = dbus_codegen::GenOpts {
            methodtype: None,
            genericvariant: true,
            ..dbus_codegen::GenOpts::default()
        };

        let output_path = Path::new(env::var("OUT_DIR").unwrap().as_str()).join("bluez_dbus/");
        let input_path = Path::new("src/bluez/bluez_dbus/");

        std::fs::create_dir_all(&output_path).unwrap();

        generate_dbus_interfaces(
            input_path.join("bluez-dbus-introspect-manager.xml"),
            output_path.join("manager.rs"),
            &options,
        )
        .unwrap();
        generate_dbus_interfaces(
            input_path.join("bluez-dbus-introspect-adapter.xml"),
            output_path.join("adapter.rs"),
            &options,
        )
        .unwrap();
        generate_dbus_interfaces(
            input_path.join("bluez-dbus-introspect-device.xml"),
            output_path.join("device.rs"),
            &options,
        )
        .unwrap();
        generate_dbus_interfaces(
            input_path.join("bluez-dbus-introspect-gatt-service.xml"),
            output_path.join("gatt_service.rs"),
            &options,
        )
        .unwrap();
        generate_dbus_interfaces(
            input_path.join("bluez-dbus-introspect-gatt-characteristic.xml"),
            output_path.join("gatt_characteristic.rs"),
            &options,
        )
        .unwrap();
        generate_dbus_interfaces(
            input_path.join("bluez-dbus-introspect-gatt-descriptor.xml"),
            output_path.join("gatt_descriptor.rs"),
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
