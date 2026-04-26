use ::jni::{
    JNIEnv,
    errors::Result,
    objects::{JClass, JObject},
};
use std::sync::{Arc, Mutex};

macro_rules! define_fn_adapter {
    (
        fn_once: $fo:ident,
        fn_once_local: $fol:ident,
        fn_once_internal: $foi:ident,
        fn_mut: $fm:ident,
        fn_mut_local: $fml:ident,
        fn_mut_internal: $fmi:ident,
        fn: $f:ident,
        fn_local: $fl:ident,
        fn_internal: $fi:ident,
        impl_class: $ic:literal,
        doc_class: $dc:literal,
        doc_method: $dm:literal,
        doc_fn_once: $dfo:literal,
        doc_fn: $df:literal,
        doc_noop: $dnoop:literal,
        signature: $closure_name:ident: impl for<'c, 'd> Fn$args:tt -> $ret:ty,
        closure: $closure:expr,
    ) => {
        fn $foi<'a: 'b, 'b>(
            env: &'b JNIEnv<'a>,
            $closure_name: impl for<'c, 'd> FnOnce$args -> $ret + 'static,
            local: bool,
        ) -> Result<JObject<'a>> {
            let adapter = env.auto_local(fn_once_adapter(env, $closure, local)?);
            env.new_object(
                JClass::from(super::classcache::get_class($ic).unwrap().as_obj()),
                "(Lio/github/gedgygedgy/rust/ops/FnAdapter;)V",
                &[(&adapter).into()],
            )
        }

        pub fn $fo<'a: 'b, 'b>(
            env: &'b JNIEnv<'a>,
            f: impl for<'c, 'd> FnOnce$args -> $ret + Send + 'static,
        ) -> Result<JObject<'a>> {
            $foi(env, f, false)
        }

        #[allow(dead_code)]
        pub fn $fol<'a: 'b, 'b>(
            env: &'b JNIEnv<'a>,
            f: impl for<'c, 'd> FnOnce$args -> $ret + 'static,
        ) -> Result<JObject<'a>> {
            $foi(env, f, true)
        }

        fn $fmi<'a: 'b, 'b>(
            env: &'b JNIEnv<'a>,
            mut $closure_name: impl for<'c, 'd> FnMut$args -> $ret + 'static,
            local: bool,
        ) -> Result<JObject<'a>> {
            let adapter = env.auto_local(fn_mut_adapter(env, $closure, local)?);
            env.new_object(
                JClass::from(super::classcache::get_class($ic).unwrap().as_obj()),
                "(Lio/github/gedgygedgy/rust/ops/FnAdapter;)V",
                &[(&adapter).into()],
            )
        }

        #[allow(dead_code)]
        pub fn $fm<'a: 'b, 'b>(
            env: &'b JNIEnv<'a>,
            f: impl for<'c, 'd> FnMut$args -> $ret + Send + 'static,
        ) -> Result<JObject<'a>> {
            $fmi(env, f, false)
        }

        #[allow(dead_code)]
        pub fn $fml<'a: 'b, 'b>(
            env: &'b JNIEnv<'a>,
            f: impl for<'c, 'd> FnMut$args -> $ret + 'static,
        ) -> Result<JObject<'a>> {
            $fmi(env, f, true)
        }

        fn $fi<'a: 'b, 'b>(
            env: &'b JNIEnv<'a>,
            $closure_name: impl for<'c, 'd> Fn$args -> $ret + 'static,
            local: bool,
        ) -> Result<JObject<'a>> {
            let adapter = env.auto_local(fn_adapter(env, $closure, local)?);
            env.new_object(
                JClass::from(super::classcache::get_class($ic).unwrap().as_obj()),
                "(Lio/github/gedgygedgy/rust/ops/FnAdapter;)V",
                &[(&adapter).into()],
            )
        }

        #[allow(dead_code)]
        pub fn $f<'a: 'b, 'b>(
            env: &'b JNIEnv<'a>,
            f: impl for<'c, 'd> Fn$args -> $ret + Send + Sync + 'static,
        ) -> Result<JObject<'a>> {
            $fi(env, f, false)
        }

        #[allow(dead_code)]
        pub fn $fl<'a: 'b, 'b>(
            env: &'b JNIEnv<'a>,
            f: impl for<'c, 'd> Fn$args -> $ret + 'static,
        ) -> Result<JObject<'a>> {
            $fi(env, f, true)
        }
    };
}

define_fn_adapter! {
    fn_once: fn_once_runnable,
    fn_once_local: fn_once_runnable_local,
    fn_once_internal: fn_once_runnable_internal,
    fn_mut: fn_mut_runnable,
    fn_mut_local: fn_mut_runnable_local,
    fn_mut_internal: fn_mut_runnable_internal,
    fn: fn_runnable,
    fn_local: fn_runnable_local,
    fn_internal: fn_runnable_internal,
    impl_class: "io/github/gedgygedgy/rust/ops/FnRunnableImpl",
    doc_class: "io.github.gedgygedgy.rust.ops.FnRunnable",
    doc_method: "run()",
    doc_fn_once: "fn_once_runnable",
    doc_fn: "fn_runnable",
    doc_noop: "be a no-op",
    signature: f: impl for<'c, 'd> Fn(&'d JNIEnv<'c>, JObject<'c>) -> (),
    closure: move |env, _obj1, obj2, _arg1, _arg2| {
        f(env, obj2);
        JObject::null()
    },
}

define_fn_adapter! {
    fn_once: fn_once_bi_function,
    fn_once_local: fn_once_bi_function_local,
    fn_once_internal: fn_once_bi_function_internal,
    fn_mut: fn_mut_bi_function,
    fn_mut_local: fn_mut_bi_function_local,
    fn_mut_internal: fn_mut_bi_function_internal,
    fn: fn_bi_function,
    fn_local: fn_bi_function_local,
    fn_internal: fn_bi_function_internal,
    impl_class: "io/github/gedgygedgy/rust/ops/FnBiFunctionImpl",
    doc_class: "io.github.gedgygedgy.rust.ops.FnBiFunction",
    doc_method: "apply()",
    doc_fn_once: "fn_once_bi_function",
    doc_fn: "fn_bi_funciton",
    doc_noop: "return `null`",
    signature: f: impl for<'c, 'd> Fn(&'d JNIEnv<'c>, JObject<'c>, JObject<'c>, JObject<'c>) -> JObject<'c>,
    closure: move |env, _obj1, obj2, arg1, arg2| {
        f(env, obj2, arg1, arg2)
    },
}

define_fn_adapter! {
    fn_once: fn_once_function,
    fn_once_local: fn_once_function_local,
    fn_once_internal: fn_once_function_internal,
    fn_mut: fn_mut_function,
    fn_mut_local: fn_mut_function_local,
    fn_mut_internal: fn_mut_function_internal,
    fn: fn_function,
    fn_local: fn_function_local,
    fn_internal: fn_function_internal,
    impl_class: "io/github/gedgygedgy/rust/ops/FnFunctionImpl",
    doc_class: "io.github.gedgygedgy.rust.ops.FnFunction",
    doc_method: "apply()",
    doc_fn_once: "fn_once_function",
    doc_fn: "fn_function",
    doc_noop: "return `null`",
    signature: f: impl for<'c, 'd> Fn(&'d JNIEnv<'c>, JObject<'c>, JObject<'c>) -> JObject<'c>,
    closure: move |env, _obj1, obj2, arg1, _arg2| {
        f(env, obj2, arg1)
    },
}

struct SendSyncWrapper<T>(T);

unsafe impl<T> Send for SendSyncWrapper<T> {}
unsafe impl<T> Sync for SendSyncWrapper<T> {}

type FnWrapper = SendSyncWrapper<
    Arc<
        dyn for<'a, 'b> Fn(
                &'b JNIEnv<'a>,
                JObject<'a>,
                JObject<'a>,
                JObject<'a>,
                JObject<'a>,
            ) -> JObject<'a>
            + 'static,
    >,
>;

fn fn_once_adapter<'a: 'b, 'b>(
    env: &'b JNIEnv<'a>,
    f: impl for<'c, 'd> FnOnce(
        &'d JNIEnv<'c>,
        JObject<'c>,
        JObject<'c>,
        JObject<'c>,
        JObject<'c>,
    ) -> JObject<'c>
    + 'static,
    local: bool,
) -> Result<JObject<'a>> {
    let mutex = Mutex::new(Some(f));
    fn_adapter(
        env,
        move |env, obj1, obj2, arg1, arg2| {
            let f = {
                let mut guard = mutex.lock().unwrap();
                if let Some(f) = guard.take() {
                    f
                } else {
                    return JObject::null();
                }
            };
            f(env, obj1, obj2, arg1, arg2)
        },
        local,
    )
}

fn fn_mut_adapter<'a: 'b, 'b>(
    env: &'b JNIEnv<'a>,
    f: impl for<'c, 'd> FnMut(
        &'d JNIEnv<'c>,
        JObject<'c>,
        JObject<'c>,
        JObject<'c>,
        JObject<'c>,
    ) -> JObject<'c>
    + 'static,
    local: bool,
) -> Result<JObject<'a>> {
    let mutex = Mutex::new(f);
    fn_adapter(
        env,
        move |env, obj1, obj2, arg1, arg2| {
            let mut guard = mutex.lock().unwrap();
            guard(env, obj1, obj2, arg1, arg2)
        },
        local,
    )
}

fn fn_adapter<'a: 'b, 'b>(
    env: &'b JNIEnv<'a>,
    f: impl for<'c, 'd> Fn(
        &'d JNIEnv<'c>,
        JObject<'c>,
        JObject<'c>,
        JObject<'c>,
        JObject<'c>,
    ) -> JObject<'c>
    + 'static,
    local: bool,
) -> Result<JObject<'a>> {
    let arc: Arc<
        dyn for<'c, 'd> Fn(
            &'d JNIEnv<'c>,
            JObject<'c>,
            JObject<'c>,
            JObject<'c>,
            JObject<'c>,
        ) -> JObject<'c>,
    > = Arc::from(f);

    let obj = env.new_object(
        JClass::from(
            super::classcache::get_class("io/github/gedgygedgy/rust/ops/FnAdapter")
                .unwrap()
                .as_obj(),
        ),
        "(Z)V",
        &[local.into()],
    )?;
    unsafe { env.set_rust_field::<_, _, FnWrapper>(obj, "data", SendSyncWrapper(arc)) }?;
    Ok(obj)
}

pub(crate) extern "C" fn fn_adapter_call_internal<'a>(
    env: JNIEnv<'a>,
    obj1: JObject<'a>,
    obj2: JObject<'a>,
    arg1: JObject<'a>,
    arg2: JObject<'a>,
) -> JObject<'a> {
    use std::panic::AssertUnwindSafe;

    let arc = if let Ok(f) = unsafe { env.get_rust_field::<_, _, FnWrapper>(obj1, "data") } {
        AssertUnwindSafe(f.0.clone())
    } else {
        return JObject::null();
    };
    super::exceptions::throw_unwind(&env, || arc(&env, obj1, obj2, arg1, arg2))
        .unwrap_or_else(|_| JObject::null())
}

pub(crate) extern "C" fn fn_adapter_close_internal(env: JNIEnv, obj: JObject) {
    let _ = super::exceptions::throw_unwind(&env, || {
        let _ = unsafe { env.take_rust_field::<_, _, FnWrapper>(obj, "data") };
    });
}
