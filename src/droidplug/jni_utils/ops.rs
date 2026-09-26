#![allow(dead_code)]

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

/// Storage protocol for [`JFnAdapter`] objects: the Java `data` field holds a
/// pointer produced by [`Arc::into_raw`] to the adapter closure. Every call
/// clones a counted reference under the object monitor and invokes the closure
/// outside it; close zeroes the field under the monitor and drops the last
/// reference.
type FnClosure = dyn for<'a, 'b> Fn(
        &'b mut Env<'a>,
        JObject<'a>,
        JObject<'a>,
        JObject<'a>,
        JObject<'a>,
    ) -> JObject<'a>
    + 'static;

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
    let boxed: Box<FnClosure> = Box::new(f);
    let arc: Arc<Box<FnClosure>> = Arc::new(boxed);

    let class = <JFnAdapter as Reference>::lookup_class(env, &Default::default())?;
    let obj = env.new_object(&*class, jni_sig!("(Z)V"), &[local.into()])?;
    let ptr = Arc::into_raw(arc);
    env.set_field(
        &obj,
        jni_str!("data"),
        jni_sig!("J"),
        (ptr as jni::sys::jlong).into(),
    )?;
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
            let _monitor = env.lock_obj(&obj1)?;
            let ptr = env.get_field(&obj1, jni_str!("data"), jni_sig!("J"))?.j()?
                as *const Box<FnClosure>;
            if ptr.is_null() {
                return Ok(JObject::null());
            }
            let arc: Arc<Box<FnClosure>> = unsafe {
                // Safety: the `data` field only ever holds a pointer from `Arc::into_raw`, and the
                // monitor serializes against `fn_adapter_close_internal` dropping the last reference.
                Arc::increment_strong_count(ptr);
                Arc::from_raw(ptr)
            };
            drop(_monitor);
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
    env.with_env(|env| {
        let _monitor = env.lock_obj(&obj)?;
        let ptr =
            env.get_field(&obj, jni_str!("data"), jni_sig!("J"))?.j()? as *const Box<FnClosure>;
        if ptr.is_null() {
            return Ok::<(), jni::errors::Error>(());
        }
        env.set_field(
            &obj,
            jni_str!("data"),
            jni_sig!("J"),
            (0 as jni::sys::jlong).into(),
        )?;
        let arc: Arc<Box<FnClosure>> = unsafe {
            // Safety: the `data` field only ever holds a pointer from `Arc::into_raw`, and the
            // monitor serializes against `fn_adapter_call_internal` cloning a counted reference.
            Arc::from_raw(ptr)
        };
        drop(_monitor);
        drop(arc);
        Ok::<(), jni::errors::Error>(())
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}

#[cfg(test)]
mod test {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };

    use jni::{
        jni_sig, jni_str,
        objects::{Global, JObject},
    };

    use super::super::test_utils;
    use super::fn_runnable;

    const ROUNDS: usize = 256;
    const CLOSES_PER_ROUND: usize = 32;

    type Round = (Global<JObject<'static>>, Arc<AtomicBool>);

    #[test]
    fn wake_and_close_concurrent_stress() {
        test_utils::with_env(|_env| Ok(())).unwrap();

        let invocations = Arc::new(AtomicUsize::new(0));

        let (caller_tx, caller_rx) = mpsc::channel::<Round>();
        let (closer_tx, closer_rx) = mpsc::channel::<Round>();
        let (ack_tx, ack_rx) = mpsc::channel::<()>();
        let caller_ack_tx = ack_tx.clone();

        let caller = thread::spawn(move || {
            while let Ok((runnable, stop)) = caller_rx.recv() {
                test_utils::with_env(|env| {
                    while !stop.load(Ordering::Relaxed) {
                        env.call_method(&runnable, jni_str!("run"), jni_sig!("()V"), &[])?;
                    }
                    Ok(())
                })
                .unwrap();
                ack_tx.send(()).unwrap();
            }
        });

        let closer = thread::spawn(move || {
            while let Ok((runnable, stop)) = closer_rx.recv() {
                test_utils::with_env(|env| {
                    for _ in 0..CLOSES_PER_ROUND {
                        env.call_method(&runnable, jni_str!("close"), jni_sig!("()V"), &[])?;
                    }
                    stop.store(true, Ordering::Relaxed);
                    Ok(())
                })
                .unwrap();
                caller_ack_tx.send(()).unwrap();
            }
        });

        for _ in 0..ROUNDS {
            let stop = Arc::new(AtomicBool::new(false));
            let (caller_ref, closer_ref) = test_utils::with_env(|env| {
                let count = invocations.clone();
                let runnable = fn_runnable(env, move |_env, _obj| {
                    count.fetch_add(1, Ordering::Relaxed);
                    thread::sleep(Duration::from_micros(50));
                })?;
                let caller_ref = env.new_global_ref(&runnable)?;
                let closer_ref = env.new_global_ref(&runnable)?;
                Ok((caller_ref, closer_ref))
            })
            .unwrap();
            caller_tx.send((caller_ref, stop.clone())).unwrap();
            closer_tx.send((closer_ref, stop)).unwrap();
            ack_rx.recv().unwrap();
            ack_rx.recv().unwrap();
        }

        drop(caller_tx);
        drop(closer_tx);
        caller.join().unwrap();
        closer.join().unwrap();

        assert!(invocations.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn call_after_close_is_noop() {
        test_utils::with_env(|env| {
            let invocations = Arc::new(AtomicUsize::new(0));
            let count = invocations.clone();
            let runnable = fn_runnable(env, move |_env, _obj| {
                count.fetch_add(1, Ordering::Relaxed);
            })?;
            let global = env.new_global_ref(&runnable)?;

            env.call_method(&global, jni_str!("close"), jni_sig!("()V"), &[])?;
            env.call_method(&global, jni_str!("close"), jni_sig!("()V"), &[])?;
            for _ in 0..10 {
                env.call_method(&global, jni_str!("run"), jni_sig!("()V"), &[])?;
            }

            assert!(!env.exception_check());
            assert_eq!(invocations.load(Ordering::Relaxed), 0);
            Ok(())
        })
        .unwrap();
    }
}
