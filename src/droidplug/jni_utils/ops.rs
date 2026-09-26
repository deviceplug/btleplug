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
/// pointer into the adapter closure's `Arc` allocation, obtained from
/// [`Arc::as_ptr`] with ownership transferred to the field via
/// [`std::mem::forget`]. Every call clones a counted reference under the
/// object monitor and invokes the closure outside it; close zeroes the field
/// under the monitor and drops the field-owned reference; a call in flight
/// keeps its own.
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
    // Send/Sync are erased by FnClosure but still upheld: non-local constructors require
    // Send (+ Sync, or a Mutex wrapper), and local adapters are pinned to one thread by
    // the Java-side LocalThreadChecker on both call and close.
    #[allow(clippy::arc_with_non_send_sync)]
    let arc: Arc<Box<FnClosure>> = Arc::new(boxed);

    let class = <JFnAdapter as Reference>::lookup_class(env, &Default::default())?;
    let obj = env.new_object(&*class, jni_sig!("(Z)V"), &[local.into()])?;
    let ptr = Arc::as_ptr(&arc);
    env.set_field(
        &obj,
        jni_str!("data"),
        jni_sig!("J"),
        (ptr as jni::sys::jlong).into(),
    )?;
    // Ownership of the closure passes to the field here; only
    // `fn_adapter_close_internal` consumes the field-owned reference.
    std::mem::forget(arc);
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
                // Safety: the `data` field only ever holds a pointer into an
                // `Arc<Box<FnClosure>>` allocation created by `fn_adapter`, and the
                // monitor serializes against `fn_adapter_close_internal` dropping the
                // field-owned reference.
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
        use std::panic::{AssertUnwindSafe, catch_unwind};

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
            // Safety: the `data` field only ever holds a pointer into an
            // `Arc<Box<FnClosure>>` allocation created by `fn_adapter`, and the
            // monitor serializes against `fn_adapter_call_internal` cloning a
            // counted reference.
            Arc::from_raw(ptr)
        };
        drop(_monitor);
        let result = catch_unwind(AssertUnwindSafe(move || drop(arc)));
        if let Err(panic) = result {
            super::exceptions::throw_panic(env, panic)?;
        }
        Ok::<(), jni::errors::Error>(())
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}

#[cfg(test)]
mod test {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{
            Arc, Mutex,
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

    struct DropProbe(Arc<AtomicUsize>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn wake_and_close_concurrent_stress() {
        test_utils::with_env(|_env| Ok(())).unwrap();

        let invocations = Arc::new(AtomicUsize::new(0));

        let (caller_tx, caller_rx) = mpsc::channel::<Round>();
        let (closer_tx, closer_rx) = mpsc::channel::<Round>();
        let (caller_result_tx, caller_result_rx) = mpsc::channel::<Result<(), String>>();
        let (closer_result_tx, closer_result_rx) = mpsc::channel::<Result<(), String>>();

        let caller = thread::spawn(move || {
            while let Ok((runnable, stop)) = caller_rx.recv() {
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    test_utils::with_env(|env| {
                        while !stop.load(Ordering::Relaxed) {
                            env.call_method(&runnable, jni_str!("run"), jni_sig!("()V"), &[])?;
                        }
                        Ok(())
                    })
                }))
                .map(|result| result.map_err(|e| e.to_string()))
                .unwrap_or_else(|panic| Err(format!("worker panicked: {panic:?}")));
                stop.store(true, Ordering::Relaxed);
                if caller_result_tx.send(outcome).is_err() {
                    break;
                }
            }
        });

        let closer = thread::spawn(move || {
            while let Ok((runnable, stop)) = closer_rx.recv() {
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    test_utils::with_env(|env| {
                        for _ in 0..CLOSES_PER_ROUND {
                            env.call_method(&runnable, jni_str!("close"), jni_sig!("()V"), &[])?;
                        }
                        Ok(())
                    })
                }))
                .map(|result| result.map_err(|e| e.to_string()))
                .unwrap_or_else(|panic| Err(format!("worker panicked: {panic:?}")));
                stop.store(true, Ordering::Relaxed);
                if closer_result_tx.send(outcome).is_err() {
                    break;
                }
            }
        });

        let mut first_error: Option<String> = None;
        'rounds: for _ in 0..ROUNDS {
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

            for outcome in [caller_result_rx.recv(), closer_result_rx.recv()] {
                match outcome {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        first_error = Some(err);
                        break 'rounds;
                    }
                    Err(_) => {
                        first_error = Some("worker dropped its result channel".to_string());
                        break 'rounds;
                    }
                }
            }
        }

        drop(caller_tx);
        drop(closer_tx);
        if let Err(panic) = caller.join() {
            panic!("caller worker panicked: {panic:?}");
        }
        if let Err(panic) = closer.join() {
            panic!("closer worker panicked: {panic:?}");
        }

        if let Some(err) = first_error {
            panic!("wake_and_close_concurrent_stress worker failed: {err}");
        }
        assert!(invocations.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn close_while_call_in_flight() {
        test_utils::with_env(|_env| Ok(())).unwrap();

        let dropped = Arc::new(AtomicUsize::new(0));

        let (runner_global, closer_global, entered_rx, release_tx) = test_utils::with_env(|env| {
            let (entered_tx, entered_rx) = mpsc::channel::<()>();
            let (release_tx, release_rx) = mpsc::channel::<()>();
            let entered_tx = Mutex::new(entered_tx);
            let release_rx = Mutex::new(release_rx);
            let probe = DropProbe(dropped.clone());
            let runnable = fn_runnable(env, move |_env, _obj| {
                let _ = &probe;
                let _ = entered_tx.lock().unwrap().send(());
                let _ = release_rx.lock().unwrap().recv();
            })?;
            let runner_global = env.new_global_ref(&runnable)?;
            let closer_global = env.new_global_ref(&runnable)?;
            Ok((runner_global, closer_global, entered_rx, release_tx))
        })
        .unwrap();

        let runner = thread::spawn(move || {
            test_utils::with_env(|env| {
                env.call_method(&runner_global, jni_str!("run"), jni_sig!("()V"), &[])?;
                Ok(())
            })
            .map_err(|e| e.to_string())
        });

        entered_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("closure did not enter within 10s");

        let (close_result_tx, close_result_rx) = mpsc::channel::<Result<(), String>>();
        let closer = thread::spawn(move || {
            let result = test_utils::with_env(|env| {
                env.call_method(&closer_global, jni_str!("close"), jni_sig!("()V"), &[])?;
                Ok(())
            })
            .map_err(|e| e.to_string());
            let _ = close_result_tx.send(result);
        });

        let close_result = close_result_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("close did not return within 10s while the call was in flight");
        assert_eq!(
            close_result,
            Ok(()),
            "close failed while a call was in flight"
        );

        assert_eq!(
            dropped.load(Ordering::Relaxed),
            0,
            "closure dropped while a call was still in flight"
        );

        release_tx
            .send(())
            .expect("release gate receiver dropped before the closure returned");
        let runner_result = runner.join().expect("runner thread panicked");
        assert_eq!(runner_result, Ok(()), "run call failed");
        let _ = closer.join();

        assert_eq!(
            dropped.load(Ordering::Relaxed),
            1,
            "expected exactly one drop, after the in-flight reference was released"
        );
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
