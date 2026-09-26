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
