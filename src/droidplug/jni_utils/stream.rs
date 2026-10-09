#![allow(dead_code)]

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
    use jni::{Env, errors::Result, jni_sig, jni_str, objects::JObject};
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    fn new_queue_stream<'local>(env: &mut Env<'local>) -> Result<JObject<'local>> {
        env.new_object(
            jni_str!("io/github/gedgygedgy/rust/stream/QueueStream"),
            jni_sig!("()V"),
            &[],
        )
    }

    fn new_object<'local>(env: &mut Env<'local>) -> Result<JObject<'local>> {
        env.new_object(jni_str!("java/lang/Object"), jni_sig!("()V"), &[])
    }

    fn add(env: &mut Env, stream: &JObject, item: &JObject) -> Result<()> {
        env.call_method(
            stream,
            jni_str!("add"),
            jni_sig!("(Ljava/lang/Object;)V"),
            &[item.into()],
        )?;
        Ok(())
    }

    fn finish(env: &mut Env, stream: &JObject) -> Result<()> {
        env.call_method(stream, jni_str!("finish"), jni_sig!("()V"), &[])?;
        Ok(())
    }

    #[test]
    fn test_jstream() {
        use std::sync::Arc;

        test_utils::with_env(|env| {
            let data = Arc::new(test_utils::TestWakerData::new());
            let waker = test_utils::test_waker(&data);

            let stream_obj = new_queue_stream(env)?;
            let mut stream = JSendStream::from_env(env, &stream_obj)?;

            assert!(
                Pin::new(&mut stream)
                    .poll_next(&mut Context::from_waker(&waker))
                    .is_pending()
            );
            assert_eq!(Arc::strong_count(&data), 3);
            assert!(!data.value());

            let obj1 = new_object(env)?;
            add(env, &stream_obj, &obj1)?;
            assert_eq!(Arc::strong_count(&data), 2);
            assert!(data.value());
            data.set_value(false);

            let obj2 = new_object(env)?;
            add(env, &stream_obj, &obj2)?;
            assert!(!data.value());

            let poll = Pin::new(&mut stream).poll_next(&mut Context::from_waker(&waker));
            let Poll::Ready(Some(Ok(actual_obj1))) = poll else {
                panic!("Poll result should be ready");
            };
            assert!(env.is_same_object(actual_obj1.as_obj(), &obj1)?);

            let poll = Pin::new(&mut stream).poll_next(&mut Context::from_waker(&waker));
            let Poll::Ready(Some(Ok(actual_obj2))) = poll else {
                panic!("Poll result should be ready");
            };
            assert!(env.is_same_object(actual_obj2.as_obj(), &obj2)?);

            assert!(
                Pin::new(&mut stream)
                    .poll_next(&mut Context::from_waker(&waker))
                    .is_pending()
            );
            assert_eq!(Arc::strong_count(&data), 3);
            assert!(!data.value());

            finish(env, &stream_obj)?;
            assert_eq!(Arc::strong_count(&data), 2);
            assert!(data.value());
            data.set_value(false);

            let poll = Pin::new(&mut stream).poll_next(&mut Context::from_waker(&waker));
            assert!(
                matches!(poll, Poll::Ready(None)),
                "finished stream should end"
            );

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_jstream_await() {
        use futures::{executor::block_on, join};

        let (mut stream, stream_obj_global, obj1_global, obj2_global) =
            test_utils::with_env(|env| {
                let stream_obj = new_queue_stream(env)?;
                let stream_obj_global = env.new_global_ref(&stream_obj)?;
                let stream = JSendStream::from_env(env, &stream_obj)?;
                let obj1 = new_object(env)?;
                let obj1_global = env.new_global_ref(&obj1)?;
                let obj2 = new_object(env)?;
                let obj2_global = env.new_global_ref(&obj2)?;
                Ok((stream, stream_obj_global, obj1_global, obj2_global))
            })
            .unwrap();

        block_on(async {
            join!(
                async {
                    test_utils::with_env(|env| {
                        let s = env.new_local_ref(stream_obj_global.as_obj())?;
                        let o1 = env.new_local_ref(obj1_global.as_obj())?;
                        let o2 = env.new_local_ref(obj2_global.as_obj())?;
                        add(env, &s, &o1)?;
                        add(env, &s, &o2)?;
                        finish(env, &s)
                    })
                    .unwrap();
                },
                async {
                    use futures::StreamExt;
                    let g1 = stream.next().await.unwrap().unwrap();
                    test_utils::with_env(|env| {
                        let o1 = env.new_local_ref(obj1_global.as_obj())?;
                        assert!(env.is_same_object(g1.as_obj(), &o1)?);
                        Ok(())
                    })
                    .unwrap();

                    let g2 = stream.next().await.unwrap().unwrap();
                    test_utils::with_env(|env| {
                        let o2 = env.new_local_ref(obj2_global.as_obj())?;
                        assert!(env.is_same_object(g2.as_obj(), &o2)?);
                        Ok(())
                    })
                    .unwrap();

                    assert!(stream.next().await.is_none());
                }
            );
        });
    }

    #[test]
    fn test_jsendstream_cross_thread_await() {
        use futures::{StreamExt, executor::block_on};
        use std::sync::{Arc, Barrier, mpsc};

        let (mut stream, stream_obj_global, obj_global) = test_utils::with_env(|env| {
            let stream_obj = new_queue_stream(env)?;
            let stream_obj_global = env.new_global_ref(&stream_obj)?;
            let stream = JSendStream::from_env(env, &stream_obj)?;
            let obj = new_object(env)?;
            let obj_global = env.new_global_ref(&obj)?;
            Ok((stream, stream_obj_global, obj_global))
        })
        .unwrap();

        let barrier = Arc::new(Barrier::new(2));
        let worker_barrier = barrier.clone();
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            worker_barrier.wait();
            let actual = block_on(stream.next()).unwrap().unwrap();
            tx.send(actual).unwrap();
        });

        barrier.wait();
        test_utils::with_env(|env| {
            let stream_local = env.new_local_ref(stream_obj_global.as_obj())?;
            let obj_local = env.new_local_ref(obj_global.as_obj())?;
            add(env, &stream_local, &obj_local)
        })
        .unwrap();
        worker.join().unwrap();
        let actual = rx.recv().unwrap();
        test_utils::with_env(|env| {
            let expected = env.new_local_ref(obj_global.as_obj())?;
            assert!(env.is_same_object(actual.as_obj(), &expected)?);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_jstream_ready_poll_owns_its_item() {
        use super::super::task::{self, JPollResult};
        use super::JStreamPoll;
        use std::sync::Arc;

        test_utils::with_env(|env| {
            let data = Arc::new(test_utils::TestWakerData::new());
            let stream_obj = new_queue_stream(env)?;
            let obj = new_object(env)?;
            add(env, &stream_obj, &obj)?;
            let stream_local = env.new_local_ref(&stream_obj)?;
            let jstream = env.cast_local::<JStream>(stream_local)?;

            let jwaker = task::waker(env, test_utils::test_waker(&data))?;
            let ready = jstream.poll_next(env, &jwaker)?;
            assert!(!env.is_same_object(&ready, JObject::null())?);

            let jwaker = task::waker(env, test_utils::test_waker(&data))?;
            let second = jstream.poll_next(env, &jwaker)?;
            assert!(
                env.is_same_object(&second, JObject::null())?,
                "a second poll handed out the only item, which `ready` already owns"
            );

            let poll_result = env.cast_local::<JPollResult>(ready)?;
            let stream_poll_obj = poll_result.get(env)?;
            let stream_poll = env.cast_local::<JStreamPoll>(stream_poll_obj)?;
            let actual = stream_poll.get(env)?;
            assert!(env.is_same_object(&actual, &obj)?);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_jsendstream_drains_concurrent_adds_in_order() {
        use futures::{StreamExt, executor::block_on};
        use std::{sync::mpsc, time::Duration};

        // Enough adds that many of them overlap a drain on the other thread.
        const ITEMS: i32 = 20_000;

        let (mut stream, stream_obj_global) = test_utils::with_env(|env| {
            let stream_obj = new_queue_stream(env)?;
            let stream_obj_global = env.new_global_ref(&stream_obj)?;
            let stream = JSendStream::from_env(env, &stream_obj)?;
            Ok((stream, stream_obj_global))
        })
        .unwrap();

        let (tx, rx) = mpsc::channel();
        let consumer = std::thread::spawn(move || {
            let drained: Result<Vec<i32>> = block_on(async {
                let mut received = Vec::new();
                while let Some(item) = stream.next().await {
                    let item = item?;
                    let value = test_utils::with_env(|env| {
                        env.call_method(item.as_obj(), jni_str!("intValue"), jni_sig!("()I"), &[])?
                            .i()
                    })?;
                    received.push(value);
                }
                Ok(received)
            });
            tx.send(drained).unwrap();
        });

        test_utils::with_env(|env| {
            let stream_local = env.new_local_ref(stream_obj_global.as_obj())?;
            for i in 0..ITEMS {
                env.with_local_frame(1, |env| -> Result<()> {
                    let boxed = env
                        .call_static_method(
                            jni_str!("java/lang/Integer"),
                            jni_str!("valueOf"),
                            jni_sig!("(I)Ljava/lang/Integer;"),
                            &[i.into()],
                        )?
                        .l()?;
                    add(env, &stream_local, &boxed)
                })?;
            }
            finish(env, &stream_local)
        })
        .unwrap();

        let received = rx
            .recv_timeout(Duration::from_secs(60))
            .expect("consumer did not finish draining the stream")
            .unwrap();
        consumer.join().unwrap();
        assert!(
            received.iter().copied().eq(0..ITEMS),
            "expected 0..{ITEMS} in order, drained {} items",
            received.len()
        );
    }
}
