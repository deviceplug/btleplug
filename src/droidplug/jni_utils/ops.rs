use ::jni::errors::ThrowRuntimeExAndDefault;
use ::jni::{
    Env, EnvUnowned, bind_java_type,
    errors::Result,
    jni_sig, jni_str,
    objects::{JObject, Reference},
};
use std::sync::{Arc, Mutex};

bind_java_type! {
    pub JFnAdapter => io.github.gedgygedgy.rust.ops.FnAdapter,
}

bind_java_type! {
    pub JFnRunnableImpl => io.github.gedgygedgy.rust.ops.FnRunnableImpl,
}

bind_java_type! {
    pub JFnBiFunctionImpl => io.github.gedgygedgy.rust.ops.FnBiFunctionImpl,
}

bind_java_type! {
    pub JFnFunctionImpl => io.github.gedgygedgy.rust.ops.FnFunctionImpl,
}

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
        impl_type: $it:ty,
        doc_class: $dc:literal,
        doc_method: $dm:literal,
        doc_fn_once: $dfo:literal,
        doc_fn: $df:literal,
        doc_noop: $dnoop:literal,
        signature: $closure_name:ident: impl for<'c, 'd> Fn$args:tt -> $ret:ty,
        closure: $closure:expr,
    ) => {
        #[allow(clippy::unused_unit)]
        fn $foi<'local>(
            env: &mut Env<'local>,
            $closure_name: impl for<'c, 'd> FnOnce$args -> $ret + 'static,
            local: bool,
        ) -> Result<JObject<'local>> {
            let adapter = fn_once_adapter(env, $closure, local)?;
            let class = <$it as Reference>::lookup_class(env, &Default::default())?;
            env.new_object(
                &*class,
                jni_sig!("(Lio/github/gedgygedgy/rust/ops/FnAdapter;)V"),
                &[(&adapter).into()],
            )
        }

        #[allow(clippy::unused_unit)]
        pub fn $fo<'local>(
            env: &mut Env<'local>,
            f: impl for<'c, 'd> FnOnce$args -> $ret + Send + 'static,
        ) -> Result<JObject<'local>> {
            $foi(env, f, false)
        }

        #[allow(dead_code, clippy::unused_unit)]
        pub fn $fol<'local>(
            env: &mut Env<'local>,
            f: impl for<'c, 'd> FnOnce$args -> $ret + 'static,
        ) -> Result<JObject<'local>> {
            $foi(env, f, true)
        }

        #[allow(clippy::unused_unit)]
        fn $fmi<'local>(
            env: &mut Env<'local>,
            mut $closure_name: impl for<'c, 'd> FnMut$args -> $ret + 'static,
            local: bool,
        ) -> Result<JObject<'local>> {
            let adapter = fn_mut_adapter(env, $closure, local)?;
            let class = <$it as Reference>::lookup_class(env, &Default::default())?;
            env.new_object(
                &*class,
                jni_sig!("(Lio/github/gedgygedgy/rust/ops/FnAdapter;)V"),
                &[(&adapter).into()],
            )
        }

        #[allow(dead_code, clippy::unused_unit)]
        pub fn $fm<'local>(
            env: &mut Env<'local>,
            f: impl for<'c, 'd> FnMut$args -> $ret + Send + 'static,
        ) -> Result<JObject<'local>> {
            $fmi(env, f, false)
        }

        #[allow(dead_code, clippy::unused_unit)]
        pub fn $fml<'local>(
            env: &mut Env<'local>,
            f: impl for<'c, 'd> FnMut$args -> $ret + 'static,
        ) -> Result<JObject<'local>> {
            $fmi(env, f, true)
        }

        #[allow(clippy::unused_unit)]
        fn $fi<'local>(
            env: &mut Env<'local>,
            $closure_name: impl for<'c, 'd> Fn$args -> $ret + 'static,
            local: bool,
        ) -> Result<JObject<'local>> {
            let adapter = fn_adapter(env, $closure, local)?;
            let class = <$it as Reference>::lookup_class(env, &Default::default())?;
            env.new_object(
                &*class,
                jni_sig!("(Lio/github/gedgygedgy/rust/ops/FnAdapter;)V"),
                &[(&adapter).into()],
            )
        }

        #[allow(dead_code, clippy::unused_unit)]
        pub fn $f<'local>(
            env: &mut Env<'local>,
            f: impl for<'c, 'd> Fn$args -> $ret + Send + Sync + 'static,
        ) -> Result<JObject<'local>> {
            $fi(env, f, false)
        }

        #[allow(dead_code, clippy::unused_unit)]
        pub fn $fl<'local>(
            env: &mut Env<'local>,
            f: impl for<'c, 'd> Fn$args -> $ret + 'static,
        ) -> Result<JObject<'local>> {
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
    impl_type: JFnRunnableImpl,
    doc_class: "io.github.gedgygedgy.rust.ops.FnRunnable",
    doc_method: "run()",
    doc_fn_once: "fn_once_runnable",
    doc_fn: "fn_runnable",
    doc_noop: "be a no-op",
    signature: f: impl for<'c, 'd> Fn(&'d mut Env<'c>, JObject<'c>) -> (),
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
    impl_type: JFnBiFunctionImpl,
    doc_class: "io.github.gedgygedgy.rust.ops.FnBiFunction",
    doc_method: "apply()",
    doc_fn_once: "fn_once_bi_function",
    doc_fn: "fn_bi_funciton",
    doc_noop: "return `null`",
    signature: f: impl for<'c, 'd> Fn(&'d mut Env<'c>, JObject<'c>, JObject<'c>, JObject<'c>) -> JObject<'c>,
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
    impl_type: JFnFunctionImpl,
    doc_class: "io.github.gedgygedgy.rust.ops.FnFunction",
    doc_method: "apply()",
    doc_fn_once: "fn_once_function",
    doc_fn: "fn_function",
    doc_noop: "return `null`",
    signature: f: impl for<'c, 'd> Fn(&'d mut Env<'c>, JObject<'c>, JObject<'c>) -> JObject<'c>,
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
                &'b mut Env<'a>,
                JObject<'a>,
                JObject<'a>,
                JObject<'a>,
                JObject<'a>,
            ) -> JObject<'a>
            + 'static,
    >,
>;

fn fn_once_adapter<'local>(
    env: &mut Env<'local>,
    f: impl for<'c, 'd> FnOnce(
        &'d mut Env<'c>,
        JObject<'c>,
        JObject<'c>,
        JObject<'c>,
        JObject<'c>,
    ) -> JObject<'c>
    + 'static,
    local: bool,
) -> Result<JObject<'local>> {
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

fn fn_mut_adapter<'local>(
    env: &mut Env<'local>,
    f: impl for<'c, 'd> FnMut(
        &'d mut Env<'c>,
        JObject<'c>,
        JObject<'c>,
        JObject<'c>,
        JObject<'c>,
    ) -> JObject<'c>
    + 'static,
    local: bool,
) -> Result<JObject<'local>> {
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

#[allow(clippy::type_complexity)]
fn fn_adapter<'local>(
    env: &mut Env<'local>,
    f: impl for<'c, 'd> Fn(
        &'d mut Env<'c>,
        JObject<'c>,
        JObject<'c>,
        JObject<'c>,
        JObject<'c>,
    ) -> JObject<'c>
    + 'static,
    local: bool,
) -> Result<JObject<'local>> {
    let arc: Arc<
        dyn for<'c, 'd> Fn(
            &'d mut Env<'c>,
            JObject<'c>,
            JObject<'c>,
            JObject<'c>,
            JObject<'c>,
        ) -> JObject<'c>,
    > = Arc::from(f);

    let class = <JFnAdapter as Reference>::lookup_class(env, &Default::default())?;
    let obj = env.new_object(&*class, jni_sig!("(Z)V"), &[local.into()])?;
    unsafe { env.set_rust_field::<_, _, FnWrapper>(&obj, jni_str!("data"), SendSyncWrapper(arc)) }?;
    Ok(obj)
}

pub(crate) extern "C" fn fn_adapter_call_internal<'local>(
    mut env: EnvUnowned<'local>,
    obj1: JObject<'local>,
    obj2: JObject<'local>,
    arg1: JObject<'local>,
    arg2: JObject<'local>,
) -> JObject<'local> {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    env.with_env(
        |env| -> std::result::Result<JObject<'local>, jni::errors::Error> {
            let arc = if let Ok(f) =
                unsafe { env.get_rust_field::<_, _, FnWrapper>(&obj1, jni_str!("data")) }
            {
                AssertUnwindSafe(f.0.clone())
            } else {
                return Ok(JObject::null());
            };
            match catch_unwind(AssertUnwindSafe(|| arc(env, obj1, obj2, arg1, arg2))) {
                Ok(result) => Ok(result),
                Err(panic) => {
                    let _ = super::exceptions::throw_panic(env, panic);
                    Ok(JObject::null())
                }
            }
        },
    )
    .resolve::<ThrowRuntimeExAndDefault>()
}

pub(crate) extern "C" fn fn_adapter_close_internal(mut env: EnvUnowned, obj: JObject) {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    env.with_env(|env| {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = unsafe { env.take_rust_field::<_, _, FnWrapper>(&obj, jni_str!("data")) };
        }));
        if let Err(panic) = result {
            super::exceptions::throw_panic(env, panic)?;
        }
        Ok::<(), jni::errors::Error>(())
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}
