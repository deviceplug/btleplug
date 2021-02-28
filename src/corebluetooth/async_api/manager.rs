use super::super::adapter::Adapter;
use crate::Result;

#[derive(Clone, Debug)]
pub struct Manager {}

impl Manager {
    pub async fn new() -> Result<Self> {
        Ok(Self {})
    }

    pub async fn adapters(&self) -> Result<Vec<Adapter>> {
        Ok(vec![Adapter::new()])
    }
}
