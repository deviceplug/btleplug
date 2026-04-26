use ::jni::{
    JNIEnv,
    errors::Result,
    objects::{JClass, JMethodID, JObject},
    signature::ReturnType,
};
use std::task::Waker;

/// Wraps the given waker in a `io.github.gedgygedgy.rust.task.Waker` object.
pub fn waker<'a: 'b, 'b>(env: &'b JNIEnv<'a>, waker: Waker) -> Result<JObject<'a>> {
    let runnable = super::ops::fn_once_runnable(env, |_e, _o| waker.wake())?;

    let obj = env.new_object(
        JClass::from(
            super::classcache::get_class("io/github/gedgygedgy/rust/task/Waker")
                .unwrap()
                .as_obj(),
        ),
        "(Lio/github/gedgygedgy/rust/ops/FnRunnable;)V",
        &[runnable.into()],
    )?;
    Ok(obj)
}

/// Wrapper for [`JObject`]s that implement
/// `io.github.gedgygedgy.rust.task.PollResult`.
pub struct JPollResult<'a: 'b, 'b> {
    internal: JObject<'a>,
    get: JMethodID,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> JPollResult<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let get = env.get_method_id(
            JClass::from(
                super::classcache::get_class("io/github/gedgygedgy/rust/task/PollResult")
                    .unwrap()
                    .as_obj(),
            ),
            "get",
            "()Ljava/lang/Object;",
        )?;
        Ok(Self {
            internal: obj,
            get,
            env,
        })
    }

    pub fn get(&self) -> Result<JObject<'a>> {
        self.env
            .call_method_unchecked(self.internal, self.get, ReturnType::Object, &[])?
            .l()
    }
}

impl<'a: 'b, 'b> ::std::ops::Deref for JPollResult<'a, 'b> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a: 'b, 'b> From<JPollResult<'a, 'b>> for JObject<'a> {
    fn from(other: JPollResult<'a, 'b>) -> JObject<'a> {
        other.internal
    }
}

#[cfg(test)]
mod test {
    use super::super::test_utils;
    use std::sync::Arc;

    #[test]
    fn test_waker_wake() {
        test_utils::JVM_ENV.with(|env| {
            let data = Arc::new(test_utils::TestWakerData::new());
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            let waker = test_utils::test_waker(&data);
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let jwaker = super::waker(env, waker).unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            env.call_method(jwaker, "wake", "()V", &[]).unwrap();
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), true);
            data.set_value(false);

            env.call_method(jwaker, "wake", "()V", &[]).unwrap();
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);
        });
    }

    #[test]
    fn test_waker_close_wake() {
        test_utils::JVM_ENV.with(|env| {
            let data = Arc::new(test_utils::TestWakerData::new());
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            let waker = test_utils::test_waker(&data);
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let jwaker = super::waker(env, waker).unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            env.call_method(jwaker, "close", "()V", &[]).unwrap();
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            env.call_method(jwaker, "wake", "()V", &[]).unwrap();
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);
        });
    }
}
