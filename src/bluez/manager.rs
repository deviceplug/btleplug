use super::adapter::Adapter;
use crate::Result;
use bluez_async::BluetoothSession;

#[derive(Clone, Debug)]
pub struct Manager {
    session: BluetoothSession,
}

impl Manager {
    pub async fn new() -> Result<Self> {
        let (_, session) = BluetoothSession::new().await?;
        Ok(Self { session })
    }

    pub async fn adapters(&self) -> Result<Vec<Adapter>> {
        let adapters = self.session.get_adapters().await?;
        Ok(adapters
            .into_iter()
            .map(|adapter| Adapter::new(self.session.clone(), adapter))
            .collect())
    }
}
