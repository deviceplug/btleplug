use super::adapter::Adapter;
use crate::{api, Result};

#[derive(Clone, Debug)]
pub struct Manager;

impl Manager {
    pub async fn new() -> Result<Manager> {
        Ok(Manager)
    }
}

impl api::Manager for Manager {
    type Adapter = Adapter;

    async fn adapters(&self) -> Result<Vec<Adapter>> {
        Ok(vec![super::global_adapter().clone()])
    }
}
