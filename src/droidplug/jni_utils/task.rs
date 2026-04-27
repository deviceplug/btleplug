use ::jni::{
    Env,
    errors::Result,
    objects::{JClass, JMethodID, JObject},
    signature::ReturnType,
};
use std::task::Waker;

pub fn waker<'a>(env: &mut Env<'a>, waker: Waker) -> Result<JObject<'a>> {
    let runnable = super::ops::fn_once_runnable(env, |_e, _o| waker.wake())?;

    let class = super::classcache::get_class("io/github/gedgygedgy/rust/task/Waker").unwrap();
    let obj = env.new_object(
        <&JClass>::from(class.as_obj()),
        "(Lio/github/gedgygedgy/rust/ops/FnRunnable;)V",
        &[(&runnable).into()],
    )?;
    Ok(obj)
}

pub struct JPollResult<'a> {
    internal: JObject<'a>,
    get: JMethodID,
}

impl<'a> JPollResult<'a> {
    pub fn from_env(env: &mut Env<'a>, obj: JObject<'a>) -> Result<Self> {
        let class =
            super::classcache::get_class("io/github/gedgygedgy/rust/task/PollResult").unwrap();
        let get = env.get_method_id(<&JClass>::from(class.as_obj()), "get", "()Ljava/lang/Object;")?;
        Ok(Self { internal: obj, get })
    }

    pub fn get(&self, env: &mut Env<'a>) -> Result<JObject<'a>> {
        unsafe { env.call_method_unchecked(&self.internal, self.get, ReturnType::Object, &[]) }?
            .l()
    }
}

impl<'a> ::std::ops::Deref for JPollResult<'a> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a> From<JPollResult<'a>> for JObject<'a> {
    fn from(other: JPollResult<'a>) -> JObject<'a> {
        other.internal
    }
}

#[cfg(test)]
mod test {
    use super::super::test_utils;
    use std::sync::Arc;

    #[test]
    fn test_waker_wake() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();

            let data = Arc::new(test_utils::TestWakerData::new());
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            let waker = test_utils::test_waker(&data);
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let jwaker = super::waker(env, waker).unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            env.call_method(&jwaker, "wake", "()V", &[]).unwrap();
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), true);
            data.set_value(false);

            env.call_method(&jwaker, "wake", "()V", &[]).unwrap();
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);
        });
    }

    #[test]
    fn test_waker_close_wake() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();

            let data = Arc::new(test_utils::TestWakerData::new());
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            let waker = test_utils::test_waker(&data);
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let jwaker = super::waker(env, waker).unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            env.call_method(&jwaker, "close", "()V", &[]).unwrap();
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            env.call_method(&jwaker, "wake", "()V", &[]).unwrap();
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);
        });
    }
}
