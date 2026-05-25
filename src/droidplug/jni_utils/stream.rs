use super::task::JPollResult;
use ::jni::{
    Env, JavaVM, bind_java_type,
    errors::Result,
    jni_sig, jni_str,
    objects::{Global, JObject},
};
use futures::stream::Stream;
use static_assertions::assert_impl_all;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

bind_java_type! {
    pub JStream => io.github.gedgygedgy.rust.stream.Stream,
}

impl<'local> JStream<'local> {
    pub fn poll_next(
        &self,
        env: &mut Env<'local>,
        waker: &JObject<'local>,
    ) -> Result<JObject<'local>> {
        env.call_method(
            self,
            jni_str!("pollNext"),
            jni_sig!("(Lio/github/gedgygedgy/rust/task/Waker;)Lio/github/gedgygedgy/rust/task/PollResult;"),
            &[waker.into()],
        )?.l()
    }
}

bind_java_type! {
    pub JStreamPoll => io.github.gedgygedgy.rust.stream.StreamPoll,
    methods {
        fn get() -> JObject,
    },
}

pub struct JSendStream {
    internal: Global<JObject<'static>>,
    vm: JavaVM,
}

impl JSendStream {
    pub fn new(env: &mut Env, stream: &JStream) -> Result<Self> {
        Ok(Self {
            internal: env.new_global_ref(&**stream)?,
            vm: env.get_java_vm()?,
        })
    }

    pub fn from_env(env: &mut Env, obj: &JObject) -> Result<Self> {
        Ok(Self {
            internal: env.new_global_ref(obj)?,
            vm: env.get_java_vm()?,
        })
    }

    fn poll_next_internal(
        &self,
        context: &mut Context<'_>,
    ) -> Result<Poll<Option<Result<Global<JObject<'static>>>>>> {
        self.vm.attach_current_thread(|env| {
            let jwaker = super::task::waker(env, context.waker().clone())?;
            let local = env.new_local_ref(self.internal.as_obj())?;
            let jstream = env.cast_local::<JStream>(local)?;
            let result = jstream.poll_next(env, &jwaker)?;

            if env.is_same_object(&result, JObject::null())? {
                return Ok(Poll::Pending);
            }

            let poll_result = env.cast_local::<JPollResult>(result)?;
            let stream_poll_obj = poll_result.get(env)?;

            if env.is_same_object(&stream_poll_obj, JObject::null())? {
                return Ok(Poll::Ready(None));
            }

            let stream_poll = env.cast_local::<JStreamPoll>(stream_poll_obj)?;
            let obj = stream_poll.get(env)?;
            Ok(Poll::Ready(Some(Ok(env.new_global_ref(obj)?))))
        })
    }
}

impl ::std::ops::Deref for JSendStream {
    type Target = Global<JObject<'static>>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl Stream for JSendStream {
    type Item = Result<Global<JObject<'static>>>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.poll_next_internal(context) {
            Ok(result) => result,
            Err(err) => Poll::Ready(Some(Err(err))),
        }
    }
}

assert_impl_all!(JSendStream: Send);

#[cfg(test)]
mod test {
    use super::super::test_utils;
    use super::{JSendStream, JStream};
    use futures::stream::Stream;
    use jni::{jni_sig, jni_str};
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    #[test]
    fn test_jstream() {
        use std::sync::Arc;

        test_utils::with_env(|env| {
            let data = Arc::new(test_utils::TestWakerData::new());
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            let waker = test_utils::test_waker(&data);
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let stream_obj = env
                .new_object(
                    jni_str!("io/github/gedgygedgy/rust/stream/QueueStream"),
                    jni_sig!("()V"),
                    &[],
                )
                .unwrap();
            let stream_local = env.new_local_ref(&stream_obj).unwrap();
            let jstream = env.cast_local::<JStream>(stream_local).unwrap();
            let mut stream = JSendStream::new(env, &jstream).unwrap();

            assert!(
                Pin::new(&mut stream)
                    .poll_next(&mut Context::from_waker(&waker))
                    .is_pending()
            );
            assert_eq!(Arc::strong_count(&data), 3);
            assert_eq!(data.value(), false);

            let obj1 = env
                .new_object(jni_str!("java/lang/Object"), jni_sig!("()V"), &[])
                .unwrap();
            env.call_method(
                &stream_obj,
                jni_str!("add"),
                jni_sig!("(Ljava/lang/Object;)V"),
                &[(&obj1).into()],
            )
            .unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), true);
            data.set_value(false);

            let obj2 = env
                .new_object(jni_str!("java/lang/Object"), jni_sig!("()V"), &[])
                .unwrap();
            env.call_method(
                &stream_obj,
                jni_str!("add"),
                jni_sig!("(Ljava/lang/Object;)V"),
                &[(&obj2).into()],
            )
            .unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let poll = Pin::new(&mut stream).poll_next(&mut Context::from_waker(&waker));
            if let Poll::Ready(Some(Ok(actual_obj1))) = poll {
                assert!(env.is_same_object(actual_obj1.as_obj(), &obj1).unwrap());
            } else {
                panic!("Poll result should be ready");
            }
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let poll = Pin::new(&mut stream).poll_next(&mut Context::from_waker(&waker));
            if let Poll::Ready(Some(Ok(actual_obj2))) = poll {
                assert!(env.is_same_object(actual_obj2.as_obj(), &obj2).unwrap());
            } else {
                panic!("Poll result should be ready");
            }
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            assert!(
                Pin::new(&mut stream)
                    .poll_next(&mut Context::from_waker(&waker))
                    .is_pending()
            );
            assert_eq!(Arc::strong_count(&data), 3);
            assert_eq!(data.value(), false);

            env.call_method(&stream_obj, jni_str!("finish"), jni_sig!("()V"), &[])
                .unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), true);
            data.set_value(false);

            let poll = Pin::new(&mut stream).poll_next(&mut Context::from_waker(&waker));
            if let Poll::Ready(None) = poll {
            } else {
                panic!("Poll result should be ready");
            }
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_jstream_await() {
        use futures::{executor::block_on, join};

        let (mut stream, stream_obj_global, obj1_global, obj2_global) =
            test_utils::with_env(|env| {
                let stream_obj = env
                    .new_object(
                        jni_str!("io/github/gedgygedgy/rust/stream/QueueStream"),
                        jni_sig!("()V"),
                        &[],
                    )
                    .unwrap();
                let stream_obj_global = env.new_global_ref(&stream_obj).unwrap();
                let stream_local = env.new_local_ref(&stream_obj).unwrap();
                let jstream = env.cast_local::<JStream>(stream_local).unwrap();
                let stream = JSendStream::new(env, &jstream).unwrap();
                let obj1 = env
                    .new_object(jni_str!("java/lang/Object"), jni_sig!("()V"), &[])
                    .unwrap();
                let obj1_global = env.new_global_ref(&obj1).unwrap();
                let obj2 = env
                    .new_object(jni_str!("java/lang/Object"), jni_sig!("()V"), &[])
                    .unwrap();
                let obj2_global = env.new_global_ref(&obj2).unwrap();
                Ok((stream, stream_obj_global, obj1_global, obj2_global))
            })
            .unwrap();

        block_on(async {
            join!(
                async {
                    test_utils::with_env(|env| {
                        let s = env.new_local_ref(stream_obj_global.as_obj()).unwrap();
                        let o1 = env.new_local_ref(obj1_global.as_obj()).unwrap();
                        let o2 = env.new_local_ref(obj2_global.as_obj()).unwrap();
                        env.call_method(
                            &s,
                            jni_str!("add"),
                            jni_sig!("(Ljava/lang/Object;)V"),
                            &[(&o1).into()],
                        )
                        .unwrap();
                        env.call_method(
                            &s,
                            jni_str!("add"),
                            jni_sig!("(Ljava/lang/Object;)V"),
                            &[(&o2).into()],
                        )
                        .unwrap();
                        env.call_method(&s, jni_str!("finish"), jni_sig!("()V"), &[])
                            .unwrap();
                        Ok(())
                    })
                    .unwrap();
                },
                async {
                    use futures::StreamExt;
                    let g1 = stream.next().await.unwrap().unwrap();
                    test_utils::with_env(|env| {
                        let o1 = env.new_local_ref(obj1_global.as_obj()).unwrap();
                        assert!(env.is_same_object(g1.as_obj(), &o1).unwrap());
                        Ok(())
                    })
                    .unwrap();

                    let g2 = stream.next().await.unwrap().unwrap();
                    test_utils::with_env(|env| {
                        let o2 = env.new_local_ref(obj2_global.as_obj()).unwrap();
                        assert!(env.is_same_object(g2.as_obj(), &o2).unwrap());
                        Ok(())
                    })
                    .unwrap();

                    assert!(stream.next().await.is_none());
                }
            );
        });
    }

    #[test]
    fn test_jsendstream_await() {
        use futures::{executor::block_on, join};

        let (mut stream, stream_obj_global, obj1_global, obj2_global) =
            test_utils::with_env(|env| {
                let stream_obj = env
                    .new_object(
                        jni_str!("io/github/gedgygedgy/rust/stream/QueueStream"),
                        jni_sig!("()V"),
                        &[],
                    )
                    .unwrap();
                let stream_obj_global = env.new_global_ref(&stream_obj).unwrap();
                let stream = JSendStream::from_env(env, &stream_obj).unwrap();
                let obj1 = env
                    .new_object(jni_str!("java/lang/Object"), jni_sig!("()V"), &[])
                    .unwrap();
                let obj1_global = env.new_global_ref(&obj1).unwrap();
                let obj2 = env
                    .new_object(jni_str!("java/lang/Object"), jni_sig!("()V"), &[])
                    .unwrap();
                let obj2_global = env.new_global_ref(&obj2).unwrap();
                Ok((stream, stream_obj_global, obj1_global, obj2_global))
            })
            .unwrap();

        block_on(async {
            join!(
                async {
                    test_utils::with_env(|env| {
                        let s = env.new_local_ref(stream_obj_global.as_obj()).unwrap();
                        let o1 = env.new_local_ref(obj1_global.as_obj()).unwrap();
                        let o2 = env.new_local_ref(obj2_global.as_obj()).unwrap();
                        env.call_method(
                            &s,
                            jni_str!("add"),
                            jni_sig!("(Ljava/lang/Object;)V"),
                            &[(&o1).into()],
                        )
                        .unwrap();
                        env.call_method(
                            &s,
                            jni_str!("add"),
                            jni_sig!("(Ljava/lang/Object;)V"),
                            &[(&o2).into()],
                        )
                        .unwrap();
                        env.call_method(&s, jni_str!("finish"), jni_sig!("()V"), &[])
                            .unwrap();
                        Ok(())
                    })
                    .unwrap();
                },
                async {
                    use futures::StreamExt;
                    let g1 = stream.next().await.unwrap().unwrap();
                    test_utils::with_env(|env| {
                        let o1 = env.new_local_ref(obj1_global.as_obj()).unwrap();
                        assert!(env.is_same_object(g1.as_obj(), &o1).unwrap());
                        Ok(())
                    })
                    .unwrap();

                    let g2 = stream.next().await.unwrap().unwrap();
                    test_utils::with_env(|env| {
                        let o2 = env.new_local_ref(obj2_global.as_obj()).unwrap();
                        assert!(env.is_same_object(g2.as_obj(), &o2).unwrap());
                        Ok(())
                    })
                    .unwrap();

                    assert!(stream.next().await.is_none());
                }
            );
        });
    }
}
