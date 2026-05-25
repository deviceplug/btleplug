pub mod arrays;
pub mod exceptions;
pub mod future;
pub mod ops;
pub mod stream;
pub mod task;
pub mod uuid;

#[cfg(test)]
pub(crate) mod test_utils {
    use jni::{
        Env, JavaVM, NativeMethod, jni_sig, jni_str,
        objects::{Global, JObject, Reference},
    };
    use lazy_static::lazy_static;
    use std::{
        cell::Cell,
        ffi::c_void,
        sync::{Arc, Mutex},
        task::{Wake, Waker},
    };

    fn test_init(env: &mut Env) -> jni::errors::Result<()> {
        use super::{
            future::{JFuture, JFutureException},
            ops::{JFnAdapter, JFnBiFunctionImpl, JFnFunctionImpl, JFnRunnableImpl},
            stream::{JStream, JStreamPoll},
            task::{JPollResult, JWaker},
        };

        let loader = jni::objects::LoaderContext::default();
        <JFuture as Reference>::lookup_class(env, &loader)?;
        <JFutureException as Reference>::lookup_class(env, &loader)?;
        <JFnAdapter as Reference>::lookup_class(env, &loader)?;
        <JStream as Reference>::lookup_class(env, &loader)?;
        <JStreamPoll as Reference>::lookup_class(env, &loader)?;
        <JWaker as Reference>::lookup_class(env, &loader)?;
        <JPollResult as Reference>::lookup_class(env, &loader)?;
        <JFnRunnableImpl as Reference>::lookup_class(env, &loader)?;
        <JFnBiFunctionImpl as Reference>::lookup_class(env, &loader)?;
        <JFnFunctionImpl as Reference>::lookup_class(env, &loader)?;

        let fn_adapter_class = <JFnAdapter as Reference>::lookup_class(env, &loader)?;
        unsafe {
            env.register_native_methods(
            &*fn_adapter_class,
            &[
                NativeMethod::from_raw_parts(
                    jni_str!("callInternal"),
                    jni_str!("(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"),
                    super::ops::fn_adapter_call_internal as *mut c_void,
                ),
                NativeMethod::from_raw_parts(
                    jni_str!("closeInternal"),
                    jni_str!("()V"),
                    super::ops::fn_adapter_close_internal as *mut c_void,
                ),
            ],
        )?
        };
        Ok(())
    }

    pub struct TestWakerData(Mutex<bool>);

    impl TestWakerData {
        pub fn new() -> Self {
            Self(Mutex::new(false))
        }

        pub fn value(&self) -> bool {
            *self.0.lock().unwrap()
        }

        pub fn set_value(&self, value: bool) {
            let mut guard = self.0.lock().unwrap();
            *guard = value;
        }
    }

    impl Wake for TestWakerData {
        fn wake(self: Arc<Self>) {
            Self::wake_by_ref(&self);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.set_value(true);
        }
    }

    pub fn test_waker(data: &Arc<TestWakerData>) -> Waker {
        Waker::from(data.clone())
    }

    struct GlobalJVM {
        jvm: JavaVM,
        class_loader: Global<JObject<'static>>,
    }

    thread_local! {
        static CLASS_LOADER_SET: Cell<bool> = const { Cell::new(false) };
    }

    pub fn with_env<F, T>(f: F) -> jni::errors::Result<T>
    where
        F: FnOnce(&mut Env) -> jni::errors::Result<T>,
    {
        JVM.jvm.attach_current_thread(|env| {
            if !CLASS_LOADER_SET.with(|c| c.get()) {
                let thread = env
                    .call_static_method(
                        jni_str!("java/lang/Thread"),
                        jni_str!("currentThread"),
                        jni_sig!("()Ljava/lang/Thread;"),
                        &[],
                    )?
                    .l()?;
                env.call_method(
                    &thread,
                    jni_str!("setContextClassLoader"),
                    jni_sig!("(Ljava/lang/ClassLoader;)V"),
                    &[JVM.class_loader.as_obj().into()],
                )?;
                CLASS_LOADER_SET.with(|c| c.set(true));
            }
            f(env)
        })
    }

    lazy_static! {
        static ref JVM: GlobalJVM = {
            use jni::InitArgsBuilder;
            use std::{env, path::PathBuf};

            let mut jni_utils_jar = PathBuf::from(env::current_exe().unwrap());
            jni_utils_jar.pop();
            jni_utils_jar.pop();
            jni_utils_jar.push("java");
            jni_utils_jar.push("libs");
            jni_utils_jar.push("btleplug-jni.jar");

            let classpath = format!("-Djava.class.path={}", jni_utils_jar.to_str().unwrap());
            let jvm_args = InitArgsBuilder::new().option(&classpath).build().unwrap();
            let jvm = JavaVM::new(jvm_args).unwrap();

            let class_loader = jvm
                .attach_current_thread(|env| {
                    test_init(env).unwrap();

                    let thread = env
                        .call_static_method(
                            jni_str!("java/lang/Thread"),
                            jni_str!("currentThread"),
                            jni_sig!("()Ljava/lang/Thread;"),
                            &[],
                        )
                        .unwrap()
                        .l()
                        .unwrap();
                    let class_loader = env
                        .call_method(
                            &thread,
                            jni_str!("getContextClassLoader"),
                            jni_sig!("()Ljava/lang/ClassLoader;"),
                            &[],
                        )
                        .unwrap()
                        .l()
                        .unwrap();
                    Ok::<_, jni::errors::Error>(env.new_global_ref(class_loader).unwrap())
                })
                .unwrap();

            GlobalJVM { jvm, class_loader }
        };
    }
}
