use super::adapter::Adapter;
use crate::{api, Result};
use async_trait::async_trait;

#[derive(Clone, Debug)]
pub struct Manager;

impl Manager {
    pub async fn new() -> Result<Manager> {
        Ok(Manager)
    }
}

#[async_trait]
impl api::Manager for Manager {
    type Adapter = Adapter;

    async fn adapters(&self) -> Result<Vec<Adapter>> {
        Ok(vec![super::global_adapter().clone()])
    }
}
