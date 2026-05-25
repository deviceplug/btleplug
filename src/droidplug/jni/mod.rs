pub mod objects;

use ::jni::errors::ThrowRuntimeExAndDefault;
use ::jni::{
    Env, EnvUnowned, NativeMethod, jni_str, native_method,
    objects::{JObject, Reference},
};
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
    // Seed the JavaVM singleton so JavaVM::singleton() works from any thread.
    env.get_java_vm()?;
    {
        let adapter_class =
            env.find_class(jni_str!("com/nonpolynomial/btleplug/android/impl/Adapter"))?;
        unsafe {
            env.register_native_methods(
                &adapter_class,
                &[
                    // Can't use native_method! here — JObject maps to Ljava/lang/Object; but the
                    // Java side declares the parameter as ScanResult. JNI requires exact signature match.
                    NativeMethod::from_raw_parts(
                        jni_str!("reportScanResult"),
                        jni_str!("(Landroid/bluetooth/le/ScanResult;)V"),
                        adapter_report_scan_result as *mut c_void,
                    ),
                    native_method! {
                        name = "onConnectionStateChanged",
                        sig = (addr: JString, connected: jboolean) -> (),
                        fn = adapter_on_connection_state_changed,
                    },
                ],
            )?
        };
        use super::jni_utils::{
            future::{JFuture, JFutureException},
            ops::{JFnAdapter, JFnBiFunctionImpl, JFnFunctionImpl, JFnRunnableImpl},
            stream::{JStream, JStreamPoll},
            task::{JPollResult, JWaker},
        };
        use objects::*;

        let loader = jni::objects::LoaderContext::default();
        <JPeripheral as Reference>::lookup_class(env, &loader)?;
        <JScanFilterClass as Reference>::lookup_class(env, &loader)?;
        <JNotConnectedException as Reference>::lookup_class(env, &loader)?;
        <JPermissionDeniedException as Reference>::lookup_class(env, &loader)?;
        <JUnexpectedCallbackException as Reference>::lookup_class(env, &loader)?;
        <JUnexpectedCharacteristicException as Reference>::lookup_class(env, &loader)?;
        <JNoSuchCharacteristicException as Reference>::lookup_class(env, &loader)?;
        <JNoBluetoothAdapterException as Reference>::lookup_class(env, &loader)?;
        <JFuture as Reference>::lookup_class(env, &loader)?;
        <JFutureException as Reference>::lookup_class(env, &loader)?;
        <JFnAdapter as Reference>::lookup_class(env, &loader)?;
        <JStream as Reference>::lookup_class(env, &loader)?;
        <JStreamPoll as Reference>::lookup_class(env, &loader)?;
        <JWaker as Reference>::lookup_class(env, &loader)?;
        <JPollResult as Reference>::lookup_class(env, &loader)?;
        <JFnRunnableImpl as Reference>::lookup_class(env, &loader)?;
        <JFnBiFunctionImpl as Reference>::lookup_class(env, &loader)?;
        <JFnFunctionImpl as Reference>::lookup_class(env, &loader)?;

        // FnAdapter native method registration
        let fn_adapter_class = <JFnAdapter as Reference>::lookup_class(env, &loader)?;
        unsafe {
            env.register_native_methods(
            &*fn_adapter_class,
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
        )?
        };
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

extern "C" fn adapter_report_scan_result<'local>(
    mut env: EnvUnowned<'local>,
    obj: JObject<'local>,
    scan_result: JObject<'local>,
) {
    env.with_env(|env| super::adapter::adapter_report_scan_result_internal(env, &obj, scan_result))
        .resolve::<ThrowRuntimeExAndDefault>();
}

fn adapter_on_connection_state_changed<'local>(
    env: &mut Env<'local>,
    obj: JObject<'local>,
    addr: JString<'local>,
    connected: jboolean,
) -> jni::errors::Result<()> {
    if let Err(e) =
        super::adapter::adapter_on_connection_state_changed_internal(env, &obj, addr, connected)
    {
        if !env.exception_check() {
            let _ = env.throw(format!("Rust error: {e}"));
        }
    }
    Ok(())
}
