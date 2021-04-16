use super::adapter::Adapter;
use crate::{api, Result};
use async_trait::async_trait;
use bluez_async::BluetoothSession;

/// Implementation of [api::Manager](crate::api::Manager).
#[derive(Clone, Debug)]
pub struct Manager {
    session: BluetoothSession,
}

impl Manager {
    pub async fn new() -> Result<Self> {
        let (_, session) = BluetoothSession::new().await?;
        Ok(Self { session })
    }
}

#[async_trait]
impl api::Manager for Manager {
    type Adapter = Adapter;

    async fn adapters(&self) -> Result<Vec<Adapter>> {
        let adapters = self.session.get_adapters().await?;
        Ok(adapters
            .into_iter()
            .map(|adapter| Adapter::new(self.session.clone(), adapter.id))
            .collect())
    }
}
