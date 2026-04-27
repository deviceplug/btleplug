pub mod objects;

use ::jni::{Env, JNIEnv, JavaVM, NativeMethod, jni_str, jni_sig, objects::JObject};
use jni::{objects::JString, sys::jboolean};
use once_cell::sync::OnceCell;
use std::ffi::c_void;

static GLOBAL_JVM: OnceCell<JavaVM> = OnceCell::new();

pub fn init(env: &mut Env) -> crate::Result<()> {
    if let Ok(()) = GLOBAL_JVM.set(env.get_java_vm()?) {
        let adapter_class =
            env.find_class(jni_str!("com/nonpolynomial/btleplug/android/impl/Adapter"))?;
        unsafe { env.register_native_methods(
            &adapter_class,
            &[
                NativeMethod {
                    name: jni_str!("reportScanResult").into(),
                    sig: jni_sig!("(Landroid/bluetooth/le/ScanResult;)V").into(),
                    fn_ptr: adapter_report_scan_result as *mut c_void,
                },
                NativeMethod {
                    name: jni_str!("onConnectionStateChanged").into(),
                    sig: jni_sig!("(Ljava/lang/String;Z)V").into(),
                    fn_ptr: adapter_on_connection_state_changed as *mut c_void,
                },
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
                NativeMethod {
                    name: jni_str!("callInternal").into(),
                    sig:
                        jni_sig!("(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;")
                            .into(),
                    fn_ptr: super::jni_utils::ops::fn_adapter_call_internal as *mut c_void,
                },
                NativeMethod {
                    name: jni_str!("closeInternal").into(),
                    sig: jni_sig!("()V").into(),
                    fn_ptr: super::jni_utils::ops::fn_adapter_close_internal as *mut c_void,
                },
            ],
        )? };

    }
    Ok(())
}

pub fn global_jvm() -> &'static JavaVM {
    GLOBAL_JVM.get().expect(
        "Droidplug has not been initialized. Please initialize it with btleplug::platform::init().",
    )
}

impl From<::jni::errors::Error> for crate::Error {
    fn from(err: ::jni::errors::Error) -> Self {
        Self::Other(Box::new(err))
    }
}

extern "C" fn adapter_report_scan_result<'a>(mut env: JNIEnv<'a>, obj: JObject, scan_result: JObject<'a>) {
    let _ = super::adapter::adapter_report_scan_result_internal(&mut env, &obj, scan_result);
}

extern "C" fn adapter_on_connection_state_changed(
    mut env: JNIEnv,
    obj: JObject,
    addr: JString,
    connected: jboolean,
) {
    let _ = super::adapter::adapter_on_connection_state_changed_internal(
        &mut env, &obj, addr, connected,
    );
}
