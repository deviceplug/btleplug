use super::task::JPollResult;
use ::jni::{
    JNIEnv, JavaVM,
    errors::{Error, Result},
    objects::{GlobalRef, JClass, JMethodID, JObject, JValue},
    signature::ReturnType,
};
use static_assertions::assert_impl_all;
use std::{
    convert::TryFrom,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Wrapper for [`JObject`]s that implement
/// `io.github.gedgygedgy.rust.future.Future`. Implements
/// [`Future`](std::future::Future) to allow asynchronous Rust code to wait for
/// a result from Java code.
///
/// For a [`Send`] version of this, use [`JSendFuture`].
pub struct JFuture<'a: 'b, 'b> {
    internal: JObject<'a>,
    poll: JMethodID,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> JFuture<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let poll = env.get_method_id(
            JClass::from(
                super::classcache::get_class("io/github/gedgygedgy/rust/future/Future")
                    .unwrap()
                    .as_obj(),
            ),
            "poll",
            "(Lio/github/gedgygedgy/rust/task/Waker;)Lio/github/gedgygedgy/rust/task/PollResult;",
        )?;
        Ok(Self {
            internal: obj,
            poll,
            env,
        })
    }

    pub fn poll(&self, waker: JObject<'a>) -> Result<JPollResult<'a, 'b>> {
        let result = self
            .env
            .call_method_unchecked(
                self.internal,
                self.poll,
                ReturnType::Object,
                &[JValue::from(waker).to_jni()],
            )?
            .l()?;
        JPollResult::from_env(self.env, result)
    }

    pub fn into_future(self) -> JFutureIntoFuture<'a, 'b> {
        JFutureIntoFuture(self)
    }
}

impl<'a: 'b, 'b> ::std::ops::Deref for JFuture<'a, 'b> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a: 'b, 'b> From<JFuture<'a, 'b>> for JObject<'a> {
    fn from(other: JFuture<'a, 'b>) -> JObject<'a> {
        other.internal
    }
}

pub struct JFutureIntoFuture<'a: 'b, 'b>(JFuture<'a, 'b>);

impl<'a: 'b, 'b> JFutureIntoFuture<'a, 'b> {
    fn poll_internal(&self, context: &mut Context<'_>) -> Result<Poll<JPollResult<'a, 'b>>> {
        use super::task::waker;
        let result = self.0.poll(waker(self.0.env, context.waker().clone())?)?;
        Ok(
            if self.0.env.is_same_object(result.clone(), JObject::null())? {
                Poll::Pending
            } else {
                Poll::Ready(result)
            },
        )
    }
}

impl<'a: 'b, 'b> Future for JFutureIntoFuture<'a, 'b> {
    type Output = Result<JPollResult<'a, 'b>>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        match self.poll_internal(context) {
            Ok(Poll::Ready(result)) => Poll::Ready(Ok(result)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(err) => Poll::Ready(Err(err)),
        }
    }
}

impl<'a: 'b, 'b> From<JFutureIntoFuture<'a, 'b>> for JFuture<'a, 'b> {
    fn from(fut: JFutureIntoFuture<'a, 'b>) -> Self {
        fut.0
    }
}

impl<'a: 'b, 'b> std::ops::Deref for JFutureIntoFuture<'a, 'b> {
    type Target = JFuture<'a, 'b>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// [`Send`] version of [`JFuture`].
pub struct JSendFuture {
    internal: GlobalRef,
    vm: JavaVM,
}

impl<'a: 'b, 'b> TryFrom<JFuture<'a, 'b>> for JSendFuture {
    type Error = Error;

    fn try_from(future: JFuture<'a, 'b>) -> Result<Self> {
        Ok(Self {
            internal: future.env.new_global_ref(future.internal)?,
            vm: future.env.get_java_vm()?,
        })
    }
}

impl ::std::ops::Deref for JSendFuture {
    type Target = GlobalRef;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl JSendFuture {
    fn poll_internal(&self, context: &mut Context<'_>) -> Result<Poll<Result<GlobalRef>>> {
        let env = self.vm.get_env()?;
        let jfuture = JFuture::from_env(&env, self.internal.as_obj())?.into_future();
        jfuture
            .poll_internal(context)
            .map(|result| result.map(|result| Ok(env.new_global_ref(result)?)))
    }
}

impl Future for JSendFuture {
    type Output = Result<GlobalRef>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        match self.poll_internal(context) {
            Ok(result) => result,
            Err(err) => Poll::Ready(Err(err)),
        }
    }
}

assert_impl_all!(JSendFuture: Send);

#[cfg(test)]
mod test {
    use super::super::{task::JPollResult, test_utils};
    use super::{JFuture, JSendFuture};
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    };

    #[test]
    fn test_jfuture() {
        use std::sync::Arc;

        test_utils::JVM_ENV.with(|env| {
            let data = Arc::new(test_utils::TestWakerData::new());
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            let waker = test_utils::test_waker(&data);
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let future_obj = env
                .new_object("io/github/gedgygedgy/rust/future/SimpleFuture", "()V", &[])
                .unwrap();
            let mut future = JFuture::from_env(env, future_obj).unwrap().into_future();

            assert!(
                Future::poll(Pin::new(&mut future), &mut Context::from_waker(&waker)).is_pending()
            );
            assert_eq!(Arc::strong_count(&data), 3);
            assert_eq!(data.value(), false);

            assert!(
                Future::poll(Pin::new(&mut future), &mut Context::from_waker(&waker)).is_pending()
            );
            assert_eq!(Arc::strong_count(&data), 3);
            assert_eq!(data.value(), false);

            let obj = env.new_object("java/lang/Object", "()V", &[]).unwrap();
            env.call_method(future_obj, "wake", "(Ljava/lang/Object;)V", &[obj.into()])
                .unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), true);

            let poll = Future::poll(Pin::new(&mut future), &mut Context::from_waker(&waker));
            if let Poll::Ready(result) = poll {
                assert!(
                    env.is_same_object(result.unwrap().get().unwrap(), obj)
                        .unwrap()
                );
            } else {
                panic!("Poll result should be ready");
            }
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), true);

            let poll = Future::poll(Pin::new(&mut future), &mut Context::from_waker(&waker));
            if let Poll::Ready(result) = poll {
                assert!(
                    env.is_same_object(result.unwrap().get().unwrap(), obj)
                        .unwrap()
                );
            } else {
                panic!("Poll result should be ready");
            }
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), true);
        });
    }

    #[test]
    fn test_jfuture_await() {
        use futures::{executor::block_on, join};

        test_utils::JVM_ENV.with(|env| {
            let future_obj = env
                .new_object("io/github/gedgygedgy/rust/future/SimpleFuture", "()V", &[])
                .unwrap();
            let future = JFuture::from_env(env, future_obj).unwrap();
            let obj = env.new_object("java/lang/Object", "()V", &[]).unwrap();

            block_on(async {
                join!(
                    async {
                        env.call_method(future_obj, "wake", "(Ljava/lang/Object;)V", &[obj.into()])
                            .unwrap();
                    },
                    async {
                        assert!(
                            env.is_same_object(
                                future.into_future().await.unwrap().get().unwrap(),
                                obj
                            )
                            .unwrap()
                        );
                    }
                );
            });
        });
    }

    #[test]
    fn test_jfuture_await_throw() {
        use futures::{executor::block_on, join};

        test_utils::JVM_ENV.with(|env| {
            let future_obj = env
                .new_object("io/github/gedgygedgy/rust/future/SimpleFuture", "()V", &[])
                .unwrap();
            let future = JFuture::from_env(env, future_obj).unwrap();
            let ex = env.new_object("java/lang/Exception", "()V", &[]).unwrap();

            block_on(async {
                join!(
                    async {
                        env.call_method(
                            future_obj,
                            "wakeWithThrowable",
                            "(Ljava/lang/Throwable;)V",
                            &[ex.into()],
                        )
                        .unwrap();
                    },
                    async {
                        future.into_future().await.unwrap().get().unwrap_err();
                        let future_ex = env.exception_occurred().unwrap();
                        env.exception_clear().unwrap();
                        let actual_ex = env
                            .call_method(future_ex, "getCause", "()Ljava/lang/Throwable;", &[])
                            .unwrap()
                            .l()
                            .unwrap();
                        assert!(env.is_same_object(actual_ex, ex).unwrap());
                    }
                );
            });
        });
    }

    #[test]
    fn test_jsendfuture_await() {
        use futures::{executor::block_on, join};
        use std::convert::TryInto;

        test_utils::JVM_ENV.with(|env| {
            let future_obj = env
                .new_object("io/github/gedgygedgy/rust/future/SimpleFuture", "()V", &[])
                .unwrap();
            let future = JFuture::from_env(env, future_obj).unwrap();
            let future: JSendFuture = future.try_into().unwrap();
            let obj = env.new_object("java/lang/Object", "()V", &[]).unwrap();

            block_on(async {
                join!(
                    async {
                        env.call_method(future_obj, "wake", "(Ljava/lang/Object;)V", &[obj.into()])
                            .unwrap();
                    },
                    async {
                        let global_ref = future.await.unwrap();
                        let jpoll = JPollResult::from_env(env, global_ref.as_obj()).unwrap();
                        assert!(env.is_same_object(jpoll.get().unwrap(), obj).unwrap());
                    }
                );
            });
        });
    }
}
