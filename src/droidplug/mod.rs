pub mod adapter;
pub mod manager;
pub mod peripheral;

use ::jni::JNIEnv;
use once_cell::sync::OnceCell;

mod jni;

static GLOBAL_ADAPTER: OnceCell<adapter::Adapter> = OnceCell::new();

pub fn init(env: &JNIEnv) -> crate::Result<()> {
    self::jni::init(env)?;
    GLOBAL_ADAPTER.get_or_try_init(|| adapter::Adapter::new())?;
    Ok(())
}

pub fn global_adapter() -> &'static adapter::Adapter {
    GLOBAL_ADAPTER.get().expect(
        "Droidplug has not been initialized. Please initialize it with btleplug::platform::init().",
    )
}
