use crate::{Error, Result};
use js_sys::{Error as JsError, Promise};
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

pub async fn wrap_promise<T: From<JsValue>>(promise: Promise) -> Result<T> {
    match JsFuture::from(promise).await {
        Ok(value) => Ok(T::from(value)),
        Err(err) => Err(Error::JavaScript(JsError::from(err).message().into())),
    }
}

pub fn uuid_from_string(uuid: String) -> Uuid {
    Uuid::parse_str(&uuid).unwrap()
}
