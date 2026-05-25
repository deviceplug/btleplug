use ::jni::{
    Env, JavaVM, bind_java_type,
    errors::Result,
    jni_sig, jni_str,
    objects::{Global, JObject},
};
use static_assertions::assert_impl_all;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

bind_java_type! {
    pub JFuture => io.github.gedgygedgy.rust.future.Future,
}

impl<'local> JFuture<'local> {
    pub fn poll(&self, env: &mut Env<'local>, waker: &JObject<'local>) -> Result<JObject<'local>> {
        env.call_method(
            self,
            jni_str!("poll"),
            jni_sig!("(Lio/github/gedgygedgy/rust/task/Waker;)Lio/github/gedgygedgy/rust/task/PollResult;"),
            &[waker.into()],
        )?.l()
    }
}

bind_java_type! {
    pub JFutureException => io.github.gedgygedgy.rust.future.FutureException,
}

pub struct JSendFuture {
    internal: Global<JObject<'static>>,
    vm: JavaVM,
}

impl JSendFuture {
    pub fn new(env: &mut Env, future: &JFuture) -> Result<Self> {
        Ok(Self {
            internal: env.new_global_ref(&**future)?,
            vm: env.get_java_vm()?,
        })
    }

    pub fn from_env(env: &mut Env, obj: &JObject) -> Result<Self> {
        Ok(Self {
            internal: env.new_global_ref(obj)?,
            vm: env.get_java_vm()?,
        })
    }

    fn poll_internal(
        &self,
        context: &mut Context<'_>,
    ) -> Result<Poll<Result<Global<JObject<'static>>>>> {
        self.vm.attach_current_thread(|env| {
            let jwaker = super::task::waker(env, context.waker().clone())?;
            let local = env.new_local_ref(self.internal.as_obj())?;
            let jfuture = env.cast_local::<JFuture>(local)?;
            let result = jfuture.poll(env, &jwaker)?;
            Ok(if env.is_same_object(&result, JObject::null())? {
                Poll::Pending
            } else {
                Poll::Ready(Ok(env.new_global_ref(result)?))
            })
        })
    }
}

impl ::std::ops::Deref for JSendFuture {
    type Target = Global<JObject<'static>>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl Future for JSendFuture {
    type Output = Result<Global<JObject<'static>>>;

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
    use jni::{jni_sig, jni_str};
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    };

    #[test]
    fn test_jfuture() {
        use super::super::task::JPollResult;
        use std::sync::Arc;

        test_utils::with_env(|env| {
            let data = Arc::new(test_utils::TestWakerData::new());
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            let waker = test_utils::test_waker(&data);
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let future_obj = env
                .new_object(
                    jni_str!("io/github/gedgygedgy/rust/future/SimpleFuture"),
                    jni_sig!("()V"),
                    &[],
                )
                .unwrap();
            let future_local = env.new_local_ref(&future_obj).unwrap();
            let jfuture = env.cast_local::<JFuture>(future_local).unwrap();
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

            let obj = env
                .new_object(jni_str!("java/lang/Object"), jni_sig!("()V"), &[])
                .unwrap();
            env.call_method(
                &future_obj,
                jni_str!("wake"),
                jni_sig!("(Ljava/lang/Object;)V"),
                &[(&obj).into()],
            )
            .unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), true);

            let poll = Future::poll(Pin::new(&mut future), &mut Context::from_waker(&waker));
            if let Poll::Ready(result) = poll {
                let global = result.unwrap();
                let local = env.new_local_ref(global.as_obj()).unwrap();
                let poll_result = env.cast_local::<JPollResult>(local).unwrap();
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
                let poll_result = env.cast_local::<JPollResult>(local).unwrap();
                let result_obj = poll_result.get(env).unwrap();
                assert!(env.is_same_object(&result_obj, &obj).unwrap());
            } else {
                panic!("Poll result should be ready");
            }
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), true);

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_jfuture_await() {
        use super::super::task::JPollResult;
        use futures::{executor::block_on, join};

        let (future, future_obj_global, obj_global) = test_utils::with_env(|env| {
            let future_obj = env
                .new_object(
                    jni_str!("io/github/gedgygedgy/rust/future/SimpleFuture"),
                    jni_sig!("()V"),
                    &[],
                )
                .unwrap();
            let future_obj_global = env.new_global_ref(&future_obj).unwrap();
            let future_local = env.new_local_ref(&future_obj).unwrap();
            let jfuture = env.cast_local::<JFuture>(future_local).unwrap();
            let future = JSendFuture::new(env, &jfuture).unwrap();
            let obj = env
                .new_object(jni_str!("java/lang/Object"), jni_sig!("()V"), &[])
                .unwrap();
            let obj_global = env.new_global_ref(&obj).unwrap();
            Ok((future, future_obj_global, obj_global))
        })
        .unwrap();

        block_on(async {
            join!(
                async {
                    test_utils::with_env(|env| {
                        let future_local = env.new_local_ref(future_obj_global.as_obj()).unwrap();
                        let obj_local = env.new_local_ref(obj_global.as_obj()).unwrap();
                        env.call_method(
                            &future_local,
                            jni_str!("wake"),
                            jni_sig!("(Ljava/lang/Object;)V"),
                            &[(&obj_local).into()],
                        )
                        .unwrap();
                        Ok(())
                    })
                    .unwrap();
                },
                async {
                    let global = future.await.unwrap();
                    test_utils::with_env(|env| {
                        let local = env.new_local_ref(global.as_obj()).unwrap();
                        let poll_result = env.cast_local::<JPollResult>(local).unwrap();
                        let result_obj = poll_result.get(env).unwrap();
                        let obj_local = env.new_local_ref(obj_global.as_obj()).unwrap();
                        assert!(env.is_same_object(&result_obj, &obj_local).unwrap());
                        Ok(())
                    })
                    .unwrap();
                }
            );
        });
    }

    #[test]
    fn test_jfuture_await_throw() {
        use futures::{executor::block_on, join};

        let (future, future_obj_global, ex_global) = test_utils::with_env(|env| {
            let future_obj = env
                .new_object(
                    jni_str!("io/github/gedgygedgy/rust/future/SimpleFuture"),
                    jni_sig!("()V"),
                    &[],
                )
                .unwrap();
            let future_obj_global = env.new_global_ref(&future_obj).unwrap();
            let future_local = env.new_local_ref(&future_obj).unwrap();
            let jfuture = env.cast_local::<JFuture>(future_local).unwrap();
            let future = JSendFuture::new(env, &jfuture).unwrap();
            let ex = env
                .new_object(jni_str!("java/lang/Exception"), jni_sig!("()V"), &[])
                .unwrap();
            let ex_global = env.new_global_ref(&ex).unwrap();
            Ok((future, future_obj_global, ex_global))
        })
        .unwrap();

        block_on(async {
            join!(
                async {
                    test_utils::with_env(|env| {
                        let future_local = env.new_local_ref(future_obj_global.as_obj()).unwrap();
                        let ex_local = env.new_local_ref(ex_global.as_obj()).unwrap();
                        env.call_method(
                            &future_local,
                            jni_str!("wakeWithThrowable"),
                            jni_sig!("(Ljava/lang/Throwable;)V"),
                            &[(&ex_local).into()],
                        )
                        .unwrap();
                        Ok(())
                    })
                    .unwrap();
                },
                async {
                    use super::super::task::JPollResult;

                    let global = future.await.unwrap();
                    test_utils::with_env(|env| {
                        let local = env.new_local_ref(global.as_obj()).unwrap();
                        let poll_result = env.cast_local::<JPollResult>(local).unwrap();
                        let _err = poll_result.get(env).unwrap_err();

                        let future_ex = env.exception_occurred().unwrap();
                        env.exception_clear();
                        let actual_ex = env
                            .call_method(
                                &future_ex,
                                jni_str!("getCause"),
                                jni_sig!("()Ljava/lang/Throwable;"),
                                &[],
                            )
                            .unwrap()
                            .l()
                            .unwrap();
                        let ex_local = env.new_local_ref(ex_global.as_obj()).unwrap();
                        assert!(env.is_same_object(&actual_ex, &ex_local).unwrap());
                        Ok(())
                    })
                    .unwrap();
                }
            );
        });
    }

    #[test]
    fn test_jsendfuture_await() {
        use super::super::task::JPollResult;
        use futures::{executor::block_on, join};

        let (future, future_obj_global, obj_global) = test_utils::with_env(|env| {
            let future_obj = env
                .new_object(
                    jni_str!("io/github/gedgygedgy/rust/future/SimpleFuture"),
                    jni_sig!("()V"),
                    &[],
                )
                .unwrap();
            let future_obj_global = env.new_global_ref(&future_obj).unwrap();
            let future = JSendFuture::from_env(env, &future_obj).unwrap();
            let obj = env
                .new_object(jni_str!("java/lang/Object"), jni_sig!("()V"), &[])
                .unwrap();
            let obj_global = env.new_global_ref(&obj).unwrap();
            Ok((future, future_obj_global, obj_global))
        })
        .unwrap();

        block_on(async {
            join!(
                async {
                    test_utils::with_env(|env| {
                        let future_local = env.new_local_ref(future_obj_global.as_obj()).unwrap();
                        let obj_local = env.new_local_ref(obj_global.as_obj()).unwrap();
                        env.call_method(
                            &future_local,
                            jni_str!("wake"),
                            jni_sig!("(Ljava/lang/Object;)V"),
                            &[(&obj_local).into()],
                        )
                        .unwrap();
                        Ok(())
                    })
                    .unwrap();
                },
                async {
                    let global_ref = future.await.unwrap();
                    test_utils::with_env(|env| {
                        let local = env.new_local_ref(global_ref.as_obj()).unwrap();
                        let jpoll = env.cast_local::<JPollResult>(local).unwrap();
                        let result_obj = jpoll.get(env).unwrap();
                        let obj_local = env.new_local_ref(obj_global.as_obj()).unwrap();
                        assert!(env.is_same_object(&result_obj, &obj_local).unwrap());
                        Ok(())
                    })
                    .unwrap();
                }
            );
        });
    }
}
