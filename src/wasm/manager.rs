use super::adapter::Adapter;
use crate::{api, Result};
use async_trait::async_trait;

/// Implementation of [api::Manager](crate::api::Manager).
#[derive(Clone, Debug)]
pub struct Manager {}

impl Manager {
    pub async fn new() -> Result<Self> {
        Ok(Self {})
    }
}

#[async_trait]
impl api::Manager for Manager {
    type Adapter = Adapter;

    async fn adapters(&self) -> Result<Vec<Adapter>> {
        if let Some(adapter) = Adapter::try_new() {
            Ok(vec![adapter])
        } else {
            Ok(vec![])
        }
    }
}
