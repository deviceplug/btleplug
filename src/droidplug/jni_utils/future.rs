use ::jni::{
    JNIEnv, JavaVM,
    errors::Result,
    objects::{GlobalRef, JClass, JMethodID, JObject},
    signature::ReturnType,
    sys::jvalue,
};
use static_assertions::assert_impl_all;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Wrapper for [`JObject`]s that implement
/// `io.github.gedgygedgy.rust.future.Future`. Provides a typed interface for
/// calling the Java future's `poll` method.
///
/// For an async [`Future`](std::future::Future) implementation, convert to
/// [`JSendFuture`] via [`JSendFuture::new`].
pub struct JFuture<'a> {
    internal: JObject<'a>,
    poll_id: JMethodID,
}

impl<'a> JFuture<'a> {
    pub fn from_env(env: &mut JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class =
            super::classcache::get_class("io/github/gedgygedgy/rust/future/Future").unwrap();
        let poll_id = env.get_method_id(
            <&JClass>::from(class.as_obj()),
            "poll",
            "(Lio/github/gedgygedgy/rust/task/Waker;)Lio/github/gedgygedgy/rust/task/PollResult;",
        )?;
        Ok(Self {
            internal: obj,
            poll_id,
        })
    }

    pub fn poll(&self, env: &mut JNIEnv<'a>, waker: &JObject<'_>) -> Result<JObject<'a>> {
        let result = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.poll_id,
                ReturnType::Object,
                &[jvalue {
                    l: waker.as_raw(),
                }],
            )
        }?
        .l()?;
        Ok(result)
    }
}

impl<'a> ::std::ops::Deref for JFuture<'a> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a> From<JFuture<'a>> for JObject<'a> {
    fn from(other: JFuture<'a>) -> JObject<'a> {
        other.internal
    }
}

/// [`Send`] version of [`JFuture`]. Implements [`Future`](std::future::Future)
/// by obtaining a [`JNIEnv`] from the stored [`JavaVM`] on each poll.
pub struct JSendFuture {
    internal: GlobalRef,
    poll_id: JMethodID,
    vm: JavaVM,
}

impl JSendFuture {
    pub fn new(env: &mut JNIEnv, future: &JFuture) -> Result<Self> {
        Ok(Self {
            internal: env.new_global_ref(&future.internal)?,
            poll_id: future.poll_id,
            vm: env.get_java_vm()?,
        })
    }

    pub fn from_env(env: &mut JNIEnv, obj: &JObject) -> Result<Self> {
        let class =
            super::classcache::get_class("io/github/gedgygedgy/rust/future/Future").unwrap();
        let poll_id = env.get_method_id(
            <&JClass>::from(class.as_obj()),
            "poll",
            "(Lio/github/gedgygedgy/rust/task/Waker;)Lio/github/gedgygedgy/rust/task/PollResult;",
        )?;
        Ok(Self {
            internal: env.new_global_ref(obj)?,
            poll_id,
            vm: env.get_java_vm()?,
        })
    }

    fn poll_internal(&self, context: &mut Context<'_>) -> Result<Poll<Result<GlobalRef>>> {
        let mut env = self.vm.get_env()?;
        let jwaker = super::task::waker(&mut env, context.waker().clone())?;
        let result = unsafe {
            env.call_method_unchecked(
                self.internal.as_obj(),
                self.poll_id,
                ReturnType::Object,
                &[jvalue {
                    l: jwaker.as_raw(),
                }],
            )
        }?
        .l()?;
        Ok(if env.is_same_object(&result, JObject::null())? {
            Poll::Pending
        } else {
            Poll::Ready(Ok(env.new_global_ref(result)?))
        })
    }
}

impl ::std::ops::Deref for JSendFuture {
    type Target = GlobalRef;

    fn deref(&self) -> &Self::Target {
        &self.internal
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
    use super::super::test_utils;
    use super::{JFuture, JSendFuture};
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    };

    #[test]
    fn test_jfuture() {
        use super::super::task::JPollResult;
        use std::sync::Arc;

        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();

            let data = Arc::new(test_utils::TestWakerData::new());
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            let waker = test_utils::test_waker(&data);
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let future_obj = env
                .new_object("io/github/gedgygedgy/rust/future/SimpleFuture", "()V", &[])
                .unwrap();
            let future_local = env.new_local_ref(&future_obj).unwrap();
            let jfuture = JFuture::from_env(env, future_local).unwrap();
            let mut future = JSendFuture::new(env, &jfuture).unwrap();

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
            env.call_method(
                &future_obj,
                "wake",
                "(Ljava/lang/Object;)V",
                &[(&obj).into()],
            )
            .unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), true);

            let poll = Future::poll(Pin::new(&mut future), &mut Context::from_waker(&waker));
            if let Poll::Ready(result) = poll {
                let global = result.unwrap();
                let local = env.new_local_ref(global.as_obj()).unwrap();
                let poll_result = JPollResult::from_env(env, local).unwrap();
                let result_obj = poll_result.get(env).unwrap();
                assert!(env.is_same_object(&result_obj, &obj).unwrap());
            } else {
                panic!("Poll result should be ready");
            }
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), true);

            let poll = Future::poll(Pin::new(&mut future), &mut Context::from_waker(&waker));
            if let Poll::Ready(result) = poll {
                let global = result.unwrap();
                let local = env.new_local_ref(global.as_obj()).unwrap();
                let poll_result = JPollResult::from_env(env, local).unwrap();
                let result_obj = poll_result.get(env).unwrap();
                assert!(env.is_same_object(&result_obj, &obj).unwrap());
            } else {
                panic!("Poll result should be ready");
            }
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), true);
        });
    }

    #[test]
    fn test_jfuture_await() {
        use super::super::task::JPollResult;
        use futures::{executor::block_on, join};

        test_utils::JVM_ENV.with(|cell| {
            let (future, future_obj_global, obj_global) = {
                let env = &mut *cell.borrow_mut();
                let future_obj = env
                    .new_object("io/github/gedgygedgy/rust/future/SimpleFuture", "()V", &[])
                    .unwrap();
                let future_obj_global = env.new_global_ref(&future_obj).unwrap();
                let future_local = env.new_local_ref(&future_obj).unwrap();
                let jfuture = JFuture::from_env(env, future_local).unwrap();
                let future = JSendFuture::new(env, &jfuture).unwrap();
                let obj = env.new_object("java/lang/Object", "()V", &[]).unwrap();
                let obj_global = env.new_global_ref(&obj).unwrap();
                (future, future_obj_global, obj_global)
            };

            block_on(async {
                join!(
                    async {
                        let env = &mut *cell.borrow_mut();
                        let future_local = env.new_local_ref(future_obj_global.as_obj()).unwrap();
                        let obj_local = env.new_local_ref(obj_global.as_obj()).unwrap();
                        env.call_method(
                            &future_local,
                            "wake",
                            "(Ljava/lang/Object;)V",
                            &[(&obj_local).into()],
                        )
                        .unwrap();
                    },
                    async {
                        let global = future.await.unwrap();
                        let env = &mut *cell.borrow_mut();
                        let local = env.new_local_ref(global.as_obj()).unwrap();
                        let poll_result = JPollResult::from_env(env, local).unwrap();
                        let result_obj = poll_result.get(env).unwrap();
                        let obj_local = env.new_local_ref(obj_global.as_obj()).unwrap();
                        assert!(env.is_same_object(&result_obj, &obj_local).unwrap());
                    }
                );
            });
        });
    }

    #[test]
    fn test_jfuture_await_throw() {
        use futures::{executor::block_on, join};

        test_utils::JVM_ENV.with(|cell| {
            let (future, future_obj_global, ex_global) = {
                let env = &mut *cell.borrow_mut();
                let future_obj = env
                    .new_object("io/github/gedgygedgy/rust/future/SimpleFuture", "()V", &[])
                    .unwrap();
                let future_obj_global = env.new_global_ref(&future_obj).unwrap();
                let future_local = env.new_local_ref(&future_obj).unwrap();
                let jfuture = JFuture::from_env(env, future_local).unwrap();
                let future = JSendFuture::new(env, &jfuture).unwrap();
                let ex = env.new_object("java/lang/Exception", "()V", &[]).unwrap();
                let ex_global = env.new_global_ref(&ex).unwrap();
                (future, future_obj_global, ex_global)
            };

            block_on(async {
                join!(
                    async {
                        let env = &mut *cell.borrow_mut();
                        let future_local = env.new_local_ref(future_obj_global.as_obj()).unwrap();
                        let ex_local = env.new_local_ref(ex_global.as_obj()).unwrap();
                        env.call_method(
                            &future_local,
                            "wakeWithThrowable",
                            "(Ljava/lang/Throwable;)V",
                            &[(&ex_local).into()],
                        )
                        .unwrap();
                    },
                    async {
                        use super::super::task::JPollResult;

                        let global = future.await.unwrap();
                        let env = &mut *cell.borrow_mut();
                        let local = env.new_local_ref(global.as_obj()).unwrap();
                        let poll_result = JPollResult::from_env(env, local).unwrap();
                        let _err = poll_result.get(env).unwrap_err();

                        let future_ex = env.exception_occurred().unwrap();
                        env.exception_clear().unwrap();
                        let actual_ex = env
                            .call_method(&future_ex, "getCause", "()Ljava/lang/Throwable;", &[])
                            .unwrap()
                            .l()
                            .unwrap();
                        let ex_local = env.new_local_ref(ex_global.as_obj()).unwrap();
                        assert!(env.is_same_object(&actual_ex, &ex_local).unwrap());
                    }
                );
            });
        });
    }

    #[test]
    fn test_jsendfuture_await() {
        use super::super::task::JPollResult;
        use futures::{executor::block_on, join};

        test_utils::JVM_ENV.with(|cell| {
            let (future, future_obj_global, obj_global) = {
                let env = &mut *cell.borrow_mut();
                let future_obj = env
                    .new_object("io/github/gedgygedgy/rust/future/SimpleFuture", "()V", &[])
                    .unwrap();
                let future_obj_global = env.new_global_ref(&future_obj).unwrap();
                let future = JSendFuture::from_env(env, &future_obj).unwrap();
                let obj = env.new_object("java/lang/Object", "()V", &[]).unwrap();
                let obj_global = env.new_global_ref(&obj).unwrap();
                (future, future_obj_global, obj_global)
            };

            block_on(async {
                join!(
                    async {
                        let env = &mut *cell.borrow_mut();
                        let future_local = env.new_local_ref(future_obj_global.as_obj()).unwrap();
                        let obj_local = env.new_local_ref(obj_global.as_obj()).unwrap();
                        env.call_method(
                            &future_local,
                            "wake",
                            "(Ljava/lang/Object;)V",
                            &[(&obj_local).into()],
                        )
                        .unwrap();
                    },
                    async {
                        let global_ref = future.await.unwrap();
                        let env = &mut *cell.borrow_mut();
                        let local = env.new_local_ref(global_ref.as_obj()).unwrap();
                        let jpoll = JPollResult::from_env(env, local).unwrap();
                        let result_obj = jpoll.get(env).unwrap();
                        let obj_local = env.new_local_ref(obj_global.as_obj()).unwrap();
                        assert!(env.is_same_object(&result_obj, &obj_local).unwrap());
                    }
                );
            });
        });
    }
}
