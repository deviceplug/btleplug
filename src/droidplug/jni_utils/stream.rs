use super::task::JPollResult;
use ::jni::{
    JNIEnv, JavaVM,
    errors::{Error, Result},
    objects::{GlobalRef, JClass, JMethodID, JObject, JValue},
    signature::ReturnType,
};
use futures::stream::Stream;
use static_assertions::assert_impl_all;
use std::{
    convert::TryFrom,
    pin::Pin,
    task::{Context, Poll},
};

/// Wrapper for [`JObject`]s that implement
/// `io.github.gedgygedgy.rust.stream.Stream`.
///
/// For a [`Send`] version of this, use [`JSendStream`].
pub struct JStream<'a: 'b, 'b> {
    internal: JObject<'a>,
    poll_next: JMethodID,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> JStream<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let poll_next = env.get_method_id(
            JClass::from(
                super::classcache::get_class("io/github/gedgygedgy/rust/stream/Stream")
                    .unwrap()
                    .as_obj(),
            ),
            "pollNext",
            "(Lio/github/gedgygedgy/rust/task/Waker;)Lio/github/gedgygedgy/rust/task/PollResult;",
        )?;
        Ok(Self {
            internal: obj,
            poll_next,
            env,
        })
    }

    fn j_poll_next(&self, waker: JObject<'a>) -> Result<Poll<Option<JObject<'a>>>> {
        let result = self
            .env
            .call_method_unchecked(
                self.internal,
                self.poll_next,
                ReturnType::Object,
                &[JValue::from(waker).to_jni()],
            )?
            .l()?;
        let _auto_local = self.env.auto_local(result);
        Ok(if self.env.is_same_object(result, JObject::null())? {
            Poll::Pending
        } else {
            Poll::Ready({
                let poll = JPollResult::from_env(self.env, result)?;
                let stream_poll_obj = poll.get()?;
                if self.env.is_same_object(stream_poll_obj, JObject::null())? {
                    None
                } else {
                    let stream_poll = JStreamPoll::from_env(self.env, stream_poll_obj)?;
                    Some(stream_poll.get()?)
                }
            })
        })
    }

    fn poll_next_internal(&self, context: &mut Context) -> Result<Poll<Option<JObject<'a>>>> {
        use super::task::waker;
        self.j_poll_next(waker(self.env, context.waker().clone())?)
    }
}

impl<'a: 'b, 'b> ::std::ops::Deref for JStream<'a, 'b> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a: 'b, 'b> From<JStream<'a, 'b>> for JObject<'a> {
    fn from(other: JStream<'a, 'b>) -> JObject<'a> {
        other.internal
    }
}

impl<'a: 'b, 'b> Stream for JStream<'a, 'b> {
    type Item = Result<JObject<'a>>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context) -> Poll<Option<Self::Item>> {
        match self.poll_next_internal(context) {
            Ok(Poll::Ready(result)) => Poll::Ready(result.map(|o| Ok(o))),
            Ok(Poll::Pending) => Poll::Pending,
            Err(err) => Poll::Ready(Some(Err(err))),
        }
    }
}

/// [`Send`] version of [`JStream`].
pub struct JSendStream {
    internal: GlobalRef,
    vm: JavaVM,
}

impl<'a: 'b, 'b> TryFrom<JStream<'a, 'b>> for JSendStream {
    type Error = Error;

    fn try_from(stream: JStream<'a, 'b>) -> Result<Self> {
        Ok(Self {
            internal: stream.env.new_global_ref(stream.internal)?,
            vm: stream.env.get_java_vm()?,
        })
    }
}

impl ::std::ops::Deref for JSendStream {
    type Target = GlobalRef;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl JSendStream {
    fn poll_next_internal(
        &self,
        context: &mut Context<'_>,
    ) -> Result<Poll<Option<Result<GlobalRef>>>> {
        let env = self.vm.get_env()?;
        let jstream = JStream::from_env(&env, self.internal.as_obj())?;
        jstream
            .poll_next_internal(context)
            .map(|result| result.map(|result| result.map(|obj| env.new_global_ref(obj))))
    }
}

impl Stream for JSendStream {
    type Item = Result<GlobalRef>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.poll_next_internal(context) {
            Ok(result) => result,
            Err(err) => Poll::Ready(Some(Err(err))),
        }
    }
}

assert_impl_all!(JSendStream: Send);

struct JStreamPoll<'a: 'b, 'b> {
    internal: JObject<'a>,
    get: JMethodID,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> JStreamPoll<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let get = env.get_method_id(
            JClass::from(
                super::classcache::get_class("io/github/gedgygedgy/rust/stream/StreamPoll")
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

#[cfg(test)]
mod test {
    use super::super::test_utils;
    use super::JStream;
    use futures::stream::Stream;
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    #[test]
    fn test_jstream() {
        use std::sync::Arc;

        test_utils::JVM_ENV.with(|env| {
            let data = Arc::new(test_utils::TestWakerData::new());
            assert_eq!(Arc::strong_count(&data), 1);
            assert_eq!(data.value(), false);

            let waker = test_utils::test_waker(&data);
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let stream_obj = env
                .new_object("io/github/gedgygedgy/rust/stream/QueueStream", "()V", &[])
                .unwrap();
            let mut stream = JStream::from_env(env, stream_obj).unwrap();

            assert!(
                Pin::new(&mut stream)
                    .poll_next(&mut Context::from_waker(&waker))
                    .is_pending()
            );
            assert_eq!(Arc::strong_count(&data), 3);
            assert_eq!(data.value(), false);

            let obj1 = env.new_object("java/lang/Object", "()V", &[]).unwrap();
            env.call_method(stream_obj, "add", "(Ljava/lang/Object;)V", &[obj1.into()])
                .unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), true);
            data.set_value(false);

            let obj2 = env.new_object("java/lang/Object", "()V", &[]).unwrap();
            env.call_method(stream_obj, "add", "(Ljava/lang/Object;)V", &[obj2.into()])
                .unwrap();
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);
            data.set_value(false);

            let poll = Pin::new(&mut stream).poll_next(&mut Context::from_waker(&waker));
            if let Poll::Ready(Some(Ok(actual_obj1))) = poll {
                assert!(env.is_same_object(actual_obj1, obj1).unwrap());
            } else {
                panic!("Poll result should be ready");
            }
            assert_eq!(Arc::strong_count(&data), 2);
            assert_eq!(data.value(), false);

            let poll = Pin::new(&mut stream).poll_next(&mut Context::from_waker(&waker));
            if let Poll::Ready(Some(Ok(actual_obj2))) = poll {
                assert!(env.is_same_object(actual_obj2, obj2).unwrap());
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

            env.call_method(stream_obj, "finish", "()V", &[]).unwrap();
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

        test_utils::JVM_ENV.with(|env| {
            let stream_obj = env
                .new_object("io/github/gedgygedgy/rust/stream/QueueStream", "()V", &[])
                .unwrap();
            let mut stream = JStream::from_env(env, stream_obj).unwrap();
            let obj1 = env.new_object("java/lang/Object", "()V", &[]).unwrap();
            let obj2 = env.new_object("java/lang/Object", "()V", &[]).unwrap();

            block_on(async {
                join!(
                    async {
                        env.call_method(stream_obj, "add", "(Ljava/lang/Object;)V", &[obj1.into()])
                            .unwrap();
                        env.call_method(stream_obj, "add", "(Ljava/lang/Object;)V", &[obj2.into()])
                            .unwrap();
                        env.call_method(stream_obj, "finish", "()V", &[]).unwrap();
                    },
                    async {
                        use futures::StreamExt;
                        assert!(
                            env.is_same_object(stream.next().await.unwrap().unwrap(), obj1)
                                .unwrap()
                        );
                        assert!(
                            env.is_same_object(stream.next().await.unwrap().unwrap(), obj2)
                                .unwrap()
                        );
                        assert!(stream.next().await.is_none());
                    }
                );
            });
        });
    }

    #[test]
    fn test_jsendstream_await() {
        use super::JSendStream;
        use futures::{executor::block_on, join};
        use std::convert::TryInto;

        test_utils::JVM_ENV.with(|env| {
            let stream_obj = env
                .new_object("io/github/gedgygedgy/rust/stream/QueueStream", "()V", &[])
                .unwrap();
            let stream = JStream::from_env(env, stream_obj).unwrap();
            let mut stream: JSendStream = stream.try_into().unwrap();
            let obj1 = env.new_object("java/lang/Object", "()V", &[]).unwrap();
            let obj2 = env.new_object("java/lang/Object", "()V", &[]).unwrap();

            block_on(async {
                join!(
                    async {
                        env.call_method(stream_obj, "add", "(Ljava/lang/Object;)V", &[obj1.into()])
                            .unwrap();
                        env.call_method(stream_obj, "add", "(Ljava/lang/Object;)V", &[obj2.into()])
                            .unwrap();
                        env.call_method(stream_obj, "finish", "()V", &[]).unwrap();
                    },
                    async {
                        use futures::StreamExt;
                        assert!(
                            env.is_same_object(
                                stream.next().await.unwrap().unwrap().as_obj(),
                                obj1
                            )
                            .unwrap()
                        );
                        assert!(
                            env.is_same_object(
                                stream.next().await.unwrap().unwrap().as_obj(),
                                obj2
                            )
                            .unwrap()
                        );
                        assert!(stream.next().await.is_none());
                    }
                );
            });
        });
    }
}
