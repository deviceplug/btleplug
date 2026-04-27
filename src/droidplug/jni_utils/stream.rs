use super::task::JPollResult;
use ::jni::{
    Env, JavaVM,
    errors::Result,
    objects::{Global, JClass, JMethodID, JObject},
    signature::ReturnType,
    sys::jvalue,
};
use futures::stream::Stream;
use static_assertions::assert_impl_all;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

pub struct JStream<'a> {
    internal: JObject<'a>,
    poll_next_id: JMethodID,
}

impl<'a> JStream<'a> {
    pub fn from_env(env: &mut Env<'a>, obj: JObject<'a>) -> Result<Self> {
        let class =
            super::classcache::get_class("io/github/gedgygedgy/rust/stream/Stream").unwrap();
        let poll_next_id = env.get_method_id(
            <&JClass>::from(class.as_obj()),
            "pollNext",
            "(Lio/github/gedgygedgy/rust/task/Waker;)Lio/github/gedgygedgy/rust/task/PollResult;",
        )?;
        Ok(Self {
            internal: obj,
            poll_next_id,
        })
    }

    pub fn poll_next_with_env(
        &self,
        env: &mut Env<'a>,
        waker: &JObject<'_>,
    ) -> Result<JObject<'a>> {
        let result = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.poll_next_id,
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

impl<'a> ::std::ops::Deref for JStream<'a> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a> From<JStream<'a>> for JObject<'a> {
    fn from(other: JStream<'a>) -> JObject<'a> {
        other.internal
    }
}

pub struct JSendStream {
    internal: Global<JObject<'static>>,
    poll_next_id: JMethodID,
    vm: JavaVM,
}

impl JSendStream {
    pub fn new(env: &mut Env, stream: &JStream) -> Result<Self> {
        Ok(Self {
            internal: env.new_global_ref(&stream.internal)?,
            poll_next_id: stream.poll_next_id,
            vm: env.get_java_vm()?,
        })
    }

    pub fn from_env(env: &mut Env, obj: &JObject) -> Result<Self> {
        let class =
            super::classcache::get_class("io/github/gedgygedgy/rust/stream/Stream").unwrap();
        let poll_next_id = env.get_method_id(
            <&JClass>::from(class.as_obj()),
            "pollNext",
            "(Lio/github/gedgygedgy/rust/task/Waker;)Lio/github/gedgygedgy/rust/task/PollResult;",
        )?;
        Ok(Self {
            internal: env.new_global_ref(obj)?,
            poll_next_id,
            vm: env.get_java_vm()?,
        })
    }

    fn poll_next_internal(
        &self,
        context: &mut Context<'_>,
    ) -> Result<Poll<Option<Result<Global<JObject<'static>>>>>> {
        let mut env = self.vm.get_env()?;
        let jwaker = super::task::waker(&mut env, context.waker().clone())?;
        let result = unsafe {
            env.call_method_unchecked(
                self.internal.as_obj(),
                self.poll_next_id,
                ReturnType::Object,
                &[jvalue {
                    l: jwaker.as_raw(),
                }],
            )
        }?
        .l()?;

        if env.is_same_object(&result, JObject::null())? {
            return Ok(Poll::Pending);
        }

        let poll_result = JPollResult::from_env(&mut env, result)?;
        let stream_poll_obj = poll_result.get(&mut env)?;

        if env.is_same_object(&stream_poll_obj, JObject::null())? {
            return Ok(Poll::Ready(None));
        }

        let stream_poll = JStreamPoll::from_env(&mut env, stream_poll_obj)?;
        let obj = stream_poll.get(&mut env)?;
        Ok(Poll::Ready(Some(Ok(env.new_global_ref(obj)?))))
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

struct JStreamPoll<'a> {
    internal: JObject<'a>,
    get: JMethodID,
}

impl<'a> JStreamPoll<'a> {
    pub fn from_env(env: &mut Env<'a>, obj: JObject<'a>) -> Result<Self> {
        let class =
            super::classcache::get_class("io/github/gedgygedgy/rust/stream/StreamPoll").unwrap();
        let get = env.get_method_id(
            <&JClass>::from(class.as_obj()),
            "get",
            "()Ljava/lang/Object;",
        )?;
        Ok(Self { internal: obj, get })
    }

    pub fn get(&self, env: &mut Env<'a>) -> Result<JObject<'a>> {
        unsafe { env.call_method_unchecked(&self.internal, self.get, ReturnType::Object, &[]) }?
            .l()
    }
}

#[cfg(test)]
mod test {
    use super::super::test_utils;
    use super::{JSendStream, JStream};
    use futures::stream::Stream;
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    #[test]
    fn test_jstream() {
        use std::sync::Arc;

        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();

            let data = Arc::new(test_utils::TestWakerData::new());
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            let waker = test_utils::test_waker(&data);
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let stream_obj = env
                .new_object("io/github/gedgygedgy/rust/stream/QueueStream", "()V", &[])
                .unwrap();
            let stream_local = env.new_local_ref(&stream_obj).unwrap();
            let jstream = JStream::from_env(env, stream_local).unwrap();
            let mut stream = JSendStream::new(env, &jstream).unwrap();

            assert!(
                Pin::new(&mut stream)
                    .poll_next(&mut Context::from_waker(&waker))
                    .is_pending()
            );
            assert_eq!(Arc::strong_count(&data), 3);
            assert_eq!(data.value(), false);

            let obj1 = env.new_object("java/lang/Object", "()V", &[]).unwrap();
            env.call_method(
                &stream_obj,
                "add",
                "(Ljava/lang/Object;)V",
                &[(&obj1).into()],
            )
            .unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), true);
            data.set_value(false);

            let obj2 = env.new_object("java/lang/Object", "()V", &[]).unwrap();
            env.call_method(
                &stream_obj,
                "add",
                "(Ljava/lang/Object;)V",
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

            env.call_method(&stream_obj, "finish", "()V", &[]).unwrap();
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
        });
    }

    #[test]
    fn test_jstream_await() {
        use futures::{executor::block_on, join};

        test_utils::JVM_ENV.with(|cell| {
            let (mut stream, stream_obj_global, obj1_global, obj2_global) = {
                let env = &mut *cell.borrow_mut();
                let stream_obj = env
                    .new_object("io/github/gedgygedgy/rust/stream/QueueStream", "()V", &[])
                    .unwrap();
                let stream_obj_global = env.new_global_ref(&stream_obj).unwrap();
                let stream_local = env.new_local_ref(&stream_obj).unwrap();
                let jstream = JStream::from_env(env, stream_local).unwrap();
                let stream = JSendStream::new(env, &jstream).unwrap();
                let obj1 = env.new_object("java/lang/Object", "()V", &[]).unwrap();
                let obj1_global = env.new_global_ref(&obj1).unwrap();
                let obj2 = env.new_object("java/lang/Object", "()V", &[]).unwrap();
                let obj2_global = env.new_global_ref(&obj2).unwrap();
                (stream, stream_obj_global, obj1_global, obj2_global)
            };

            block_on(async {
                join!(
                    async {
                        let env = &mut *cell.borrow_mut();
                        let s = env.new_local_ref(stream_obj_global.as_obj()).unwrap();
                        let o1 = env.new_local_ref(obj1_global.as_obj()).unwrap();
                        let o2 = env.new_local_ref(obj2_global.as_obj()).unwrap();
                        env.call_method(
                            &s,
                            "add",
                            "(Ljava/lang/Object;)V",
                            &[(&o1).into()],
                        )
                        .unwrap();
                        env.call_method(
                            &s,
                            "add",
                            "(Ljava/lang/Object;)V",
                            &[(&o2).into()],
                        )
                        .unwrap();
                        env.call_method(&s, "finish", "()V", &[]).unwrap();
                    },
                    async {
                        use futures::StreamExt;
                        let g1 = stream.next().await.unwrap().unwrap();
                        {
                            let mut guard = cell.borrow_mut();
                            let env = &mut *guard;
                            let o1 = env.new_local_ref(obj1_global.as_obj()).unwrap();
                            assert!(env.is_same_object(g1.as_obj(), &o1).unwrap());
                        }

                        let g2 = stream.next().await.unwrap().unwrap();
                        {
                            let mut guard = cell.borrow_mut();
                            let env = &mut *guard;
                            let o2 = env.new_local_ref(obj2_global.as_obj()).unwrap();
                            assert!(env.is_same_object(g2.as_obj(), &o2).unwrap());
                        }

                        assert!(stream.next().await.is_none());
                    }
                );
            });
        });
    }

    #[test]
    fn test_jsendstream_await() {
        use futures::{executor::block_on, join};

        test_utils::JVM_ENV.with(|cell| {
            let (mut stream, stream_obj_global, obj1_global, obj2_global) = {
                let env = &mut *cell.borrow_mut();
                let stream_obj = env
                    .new_object("io/github/gedgygedgy/rust/stream/QueueStream", "()V", &[])
                    .unwrap();
                let stream_obj_global = env.new_global_ref(&stream_obj).unwrap();
                let stream = JSendStream::from_env(env, &stream_obj).unwrap();
                let obj1 = env.new_object("java/lang/Object", "()V", &[]).unwrap();
                let obj1_global = env.new_global_ref(&obj1).unwrap();
                let obj2 = env.new_object("java/lang/Object", "()V", &[]).unwrap();
                let obj2_global = env.new_global_ref(&obj2).unwrap();
                (stream, stream_obj_global, obj1_global, obj2_global)
            };

            block_on(async {
                join!(
                    async {
                        let env = &mut *cell.borrow_mut();
                        let s = env.new_local_ref(stream_obj_global.as_obj()).unwrap();
                        let o1 = env.new_local_ref(obj1_global.as_obj()).unwrap();
                        let o2 = env.new_local_ref(obj2_global.as_obj()).unwrap();
                        env.call_method(
                            &s,
                            "add",
                            "(Ljava/lang/Object;)V",
                            &[(&o1).into()],
                        )
                        .unwrap();
                        env.call_method(
                            &s,
                            "add",
                            "(Ljava/lang/Object;)V",
                            &[(&o2).into()],
                        )
                        .unwrap();
                        env.call_method(&s, "finish", "()V", &[]).unwrap();
                    },
                    async {
                        use futures::StreamExt;
                        let g1 = stream.next().await.unwrap().unwrap();
                        {
                            let mut guard = cell.borrow_mut();
                            let env = &mut *guard;
                            let o1 = env.new_local_ref(obj1_global.as_obj()).unwrap();
                            assert!(env.is_same_object(g1.as_obj(), &o1).unwrap());
                        }

                        let g2 = stream.next().await.unwrap().unwrap();
                        {
                            let mut guard = cell.borrow_mut();
                            let env = &mut *guard;
                            let o2 = env.new_local_ref(obj2_global.as_obj()).unwrap();
                            assert!(env.is_same_object(g2.as_obj(), &o2).unwrap());
                        }

                        assert!(stream.next().await.is_none());
                    }
                );
            });
        });
    }
}
