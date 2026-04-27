pub mod objects;

use ::jni::{Env, EnvUnowned, NativeMethod, jni_str, objects::JObject};
use jni::{objects::JString, sys::jboolean};
use std::ffi::c_void;
use std::sync::Once;

static INIT: Once = Once::new();

pub fn init(env: &mut Env) -> crate::Result<()> {
    let mut init_result: crate::Result<()> = Ok(());
    INIT.call_once(|| {
        if let Err(e) = init_inner(env) {
            init_result = Err(e);
        }
    });
    init_result
}

fn init_inner(env: &mut Env) -> crate::Result<()> {
    {
        let adapter_class =
            env.find_class(jni_str!("com/nonpolynomial/btleplug/android/impl/Adapter"))?;
        unsafe { env.register_native_methods(
            &adapter_class,
            &[
                NativeMethod::from_raw_parts(
                    jni_str!("reportScanResult"),
                    jni_str!("(Landroid/bluetooth/le/ScanResult;)V"),
                    adapter_report_scan_result as *mut c_void,
                ),
                NativeMethod::from_raw_parts(
                    jni_str!("onConnectionStateChanged"),
                    jni_str!("(Ljava/lang/String;Z)V"),
                    adapter_on_connection_state_changed as *mut c_void,
                ),
            ],
        )? };
        super::jni_utils::classcache::find_add_class(
            env,
            "com/nonpolynomial/btleplug/android/impl/Peripheral",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "com/nonpolynomial/btleplug/android/impl/ScanFilter",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "com/nonpolynomial/btleplug/android/impl/NotConnectedException",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "com/nonpolynomial/btleplug/android/impl/PermissionDeniedException",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "com/nonpolynomial/btleplug/android/impl/UnexpectedCallbackException",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "com/nonpolynomial/btleplug/android/impl/UnexpectedCharacteristicException",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "com/nonpolynomial/btleplug/android/impl/NoSuchCharacteristicException",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "com/nonpolynomial/btleplug/android/impl/NoBluetoothAdapterException",
        )?;

        // jni-utils class caching
        super::jni_utils::classcache::find_add_class(
            env,
            "io/github/gedgygedgy/rust/future/Future",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "io/github/gedgygedgy/rust/future/FutureException",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "io/github/gedgygedgy/rust/ops/FnAdapter",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "io/github/gedgygedgy/rust/stream/Stream",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "io/github/gedgygedgy/rust/stream/StreamPoll",
        )?;
        super::jni_utils::classcache::find_add_class(env, "io/github/gedgygedgy/rust/task/Waker")?;
        super::jni_utils::classcache::find_add_class(
            env,
            "io/github/gedgygedgy/rust/task/PollResult",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "io/github/gedgygedgy/rust/ops/FnRunnableImpl",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "io/github/gedgygedgy/rust/ops/FnBiFunctionImpl",
        )?;
        super::jni_utils::classcache::find_add_class(
            env,
            "io/github/gedgygedgy/rust/ops/FnFunctionImpl",
        )?;

        // FnAdapter native method registration
        let fn_adapter_class = env.find_class(jni_str!("io/github/gedgygedgy/rust/ops/FnAdapter"))?;
        unsafe { env.register_native_methods(
            &fn_adapter_class,
            &[
                NativeMethod::from_raw_parts(
                    jni_str!("callInternal"),
                    jni_str!("(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"),
                    super::jni_utils::ops::fn_adapter_call_internal as *mut c_void,
                ),
                NativeMethod::from_raw_parts(
                    jni_str!("closeInternal"),
                    jni_str!("()V"),
                    super::jni_utils::ops::fn_adapter_close_internal as *mut c_void,
                ),
            ],
        )? };

    }
    Ok(())
}

pub fn jvm() -> crate::Result<jni::JavaVM> {
    jni::JavaVM::singleton().map_err(|e| crate::Error::Other(Box::new(e)))
}

impl From<::jni::errors::Error> for crate::Error {
    fn from(err: ::jni::errors::Error) -> Self {
        Self::Other(Box::new(err))
    }
}

extern "C" fn adapter_report_scan_result<'a>(mut env: EnvUnowned<'a>, obj: JObject, scan_result: JObject<'a>) {
    let outcome = env.with_env(|env| {
        let _ = super::adapter::adapter_report_scan_result_internal(env, &obj, scan_result);
        Ok::<(), jni::errors::Error>(())
    });
    let _ = outcome.into_outcome();
}

extern "C" fn adapter_on_connection_state_changed(
    mut env: EnvUnowned,
    obj: JObject,
    addr: JString,
    connected: jboolean,
) {
    let outcome = env.with_env(|env| {
        let _ = super::adapter::adapter_on_connection_state_changed_internal(
            env, &obj, addr, connected,
        );
        Ok::<(), jni::errors::Error>(())
    });
    let _ = outcome.into_outcome();
}
