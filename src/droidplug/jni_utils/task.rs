use ::jni::{
    Env, bind_java_type,
    errors::Result,
    jni_sig,
    objects::{JObject, Reference},
};
use std::task::Waker;

bind_java_type! {
    pub JWaker => io.github.gedgygedgy.rust.task.Waker,
}

pub fn waker<'a>(env: &mut Env<'a>, waker: Waker) -> Result<JObject<'a>> {
    let runnable = super::ops::fn_once_runnable(env, |_e, _o| waker.wake())?;

    let class = <JWaker as Reference>::lookup_class(env, &Default::default())?;
    let obj = env.new_object(
        &*class,
        jni_sig!("(Lio/github/gedgygedgy/rust/ops/FnRunnable;)V"),
        &[(&runnable).into()],
    )?;
    Ok(obj)
}

bind_java_type! {
    pub JPollResult => io.github.gedgygedgy.rust.task.PollResult,
    methods {
        fn get() -> JObject,
    },
}

#[cfg(test)]
mod test {
    use super::super::test_utils;
    use jni::{jni_sig, jni_str};
    use std::sync::Arc;

    #[test]
    fn test_waker_wake() {
        test_utils::with_env(|env| {
            let data = Arc::new(test_utils::TestWakerData::new());
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            let waker = test_utils::test_waker(&data);
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let jwaker = super::waker(env, waker).unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            env.call_method(&jwaker, jni_str!("wake"), jni_sig!("()V"), &[])
                .unwrap();
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), true);
            data.set_value(false);

            env.call_method(&jwaker, jni_str!("wake"), jni_sig!("()V"), &[])
                .unwrap();
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_waker_close_wake() {
        test_utils::with_env(|env| {
            let data = Arc::new(test_utils::TestWakerData::new());
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            let waker = test_utils::test_waker(&data);
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let jwaker = super::waker(env, waker).unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            env.call_method(&jwaker, jni_str!("close"), jni_sig!("()V"), &[])
                .unwrap();
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            env.call_method(&jwaker, jni_str!("wake"), jni_sig!("()V"), &[])
                .unwrap();
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);
            Ok(())
        })
        .unwrap();
    }
}
