// See the "macOS permissions note" in README.md before running this on macOS
// Big Sur or later.

use btleplug::api::{
    Central, Manager as _,
};
use btleplug::platform::Manager;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let manager = Manager::new().await?;

    for adapter in manager.adapters().await.unwrap() {
        println!("Info: {:?}", adapter.adapter_info().await.unwrap());
        println!("Mac: {:?}", adapter.adapter_mac().await.unwrap());
    }

    Ok(())
}
