#[cfg(target_os = "windows")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
    use btleplug::platform::Manager;
    use std::future::IntoFuture;
    use std::time::{Duration, Instant};
    use windows::Devices::Bluetooth::{
        BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice,
    };
    use windows::Devices::Enumeration::DeviceInformation;

    let name = std::env::var("BTLEPLUG_TEST_PERIPHERAL").unwrap_or("btleplug-test".into());
    let manager = Manager::new().await?;
    let adapter = manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .ok_or("no adapter")?;
    adapter.start_scan(ScanFilter::default()).await?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let peripheral = loop {
        let mut found = None;
        for p in adapter.peripherals().await? {
            if let Some(props) = p.properties().await? {
                if props.local_name.as_deref() == Some(name.as_str()) {
                    found = Some(p);
                }
            }
        }
        if let Some(p) = found {
            break p;
        }
        if Instant::now() > deadline {
            return Err(format!("{name} not found").into());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    adapter.stop_scan().await?;
    let t = Instant::now();
    peripheral.connect().await?;
    peripheral.discover_services().await?;
    println!(
        "connected to {name} ({}) in {:?}",
        peripheral.id(),
        t.elapsed()
    );

    let t = Instant::now();
    let selector = BluetoothLEDevice::GetDeviceSelectorFromConnectionStatus(
        BluetoothConnectionStatus::Connected,
    )?;
    let devices = DeviceInformation::FindAllAsyncAqsFilter(&selector)?
        .into_future()
        .await?;
    println!(
        "FindAllAsyncAqsFilter: {} devices in {:?}",
        devices.Size()?,
        t.elapsed()
    );

    for info in devices {
        let label = format!("{} [{}]", info.Name()?, info.Id()?);
        let t = Instant::now();
        let device = BluetoothLEDevice::FromIdAsync(&info.Id()?)?
            .into_future()
            .await?;
        println!(
            "  {label}\n    FromIdAsync: {:?} (address {:012x})",
            t.elapsed(),
            device.BluetoothAddress()?
        );
        let t = Instant::now();
        match device
            .GetGattServicesWithCacheModeAsync(BluetoothCacheMode::Cached)?
            .into_future()
            .await
        {
            Ok(result) => println!(
                "    GetGattServices(Cached): {:?}, status {:?}, {} services",
                t.elapsed(),
                result.Status()?,
                result.Services()?.Size()?
            ),
            Err(e) => println!("    GetGattServices(Cached): {:?}, error {e}", t.elapsed()),
        }
    }

    peripheral.disconnect().await?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("retrieve_timing only runs on Windows");
}
