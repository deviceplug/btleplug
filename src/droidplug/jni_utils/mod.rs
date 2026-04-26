pub mod arrays;
pub mod classcache;
pub mod exceptions;
pub mod future;
pub mod ops;
pub mod stream;
pub mod task;
pub mod uuid;

#[cfg(test)]
pub(crate) mod test_utils {
    use jni::{JNIEnv, JavaVM, objects::GlobalRef};
    use lazy_static::lazy_static;
    use std::{
        cell::RefCell,
        sync::{Arc, Mutex},
        task::{Wake, Waker},
    };

    use jni::NativeMethod;

    fn test_init(env: &mut JNIEnv) -> jni::errors::Result<()> {
        use std::ffi::c_void;
        super::classcache::find_add_class(env, "io/github/gedgygedgy/rust/future/Future")?;
        super::classcache::find_add_class(env, "io/github/gedgygedgy/rust/future/FutureException")?;
        super::classcache::find_add_class(env, "io/github/gedgygedgy/rust/ops/FnAdapter")?;
        super::classcache::find_add_class(env, "io/github/gedgygedgy/rust/stream/Stream")?;
        super::classcache::find_add_class(env, "io/github/gedgygedgy/rust/stream/StreamPoll")?;
        super::classcache::find_add_class(env, "io/github/gedgygedgy/rust/task/Waker")?;
        super::classcache::find_add_class(env, "io/github/gedgygedgy/rust/task/PollResult")?;
        super::classcache::find_add_class(env, "io/github/gedgygedgy/rust/ops/FnRunnableImpl")?;
        super::classcache::find_add_class(env, "io/github/gedgygedgy/rust/ops/FnBiFunctionImpl")?;
        super::classcache::find_add_class(env, "io/github/gedgygedgy/rust/ops/FnFunctionImpl")?;

        let class = env.find_class("io/github/gedgygedgy/rust/ops/FnAdapter")?;
        env.register_native_methods(
            &class,
            &[
                NativeMethod {
                    name: "callInternal".into(),
                    sig:
                        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"
                            .into(),
                    fn_ptr: super::ops::fn_adapter_call_internal as *mut c_void,
                },
                NativeMethod {
                    name: "closeInternal".into(),
                    sig: "()V".into(),
                    fn_ptr: super::ops::fn_adapter_close_internal as *mut c_void,
                },
            ],
        )?;
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
        class_loader: GlobalRef,
    }

    thread_local! {
        pub static JVM_ENV: RefCell<JNIEnv<'static>> = {
            let mut env = JVM.jvm.attach_current_thread_permanently().unwrap();

            let thread = env
                .call_static_method(
                    "java/lang/Thread",
                    "currentThread",
                    "()Ljava/lang/Thread;",
                    &[],
                )
                .unwrap()
                .l()
                .unwrap();
            env.call_method(
                &thread,
                "setContextClassLoader",
                "(Ljava/lang/ClassLoader;)V",
                &[(&JVM.class_loader).into()]
            ).unwrap();

            RefCell::new(env)
        }
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

            let classpath = format!(
                "-Djava.class.path={}",
                jni_utils_jar.to_str().unwrap()
            );
            let jvm_args = InitArgsBuilder::new()
                .option(&classpath)
                .build()
                .unwrap();
            let jvm = JavaVM::new(jvm_args).unwrap();

            let mut env = jvm.attach_current_thread_permanently().unwrap();
            test_init(&mut env).unwrap();

            let thread = env
                .call_static_method(
                    "java/lang/Thread",
                    "currentThread",
                    "()Ljava/lang/Thread;",
                    &[],
                )
                .unwrap()
                .l()
                .unwrap();
            let class_loader = env
                .call_method(
                    &thread,
                    "getContextClassLoader",
                    "()Ljava/lang/ClassLoader;",
                    &[],
                )
                .unwrap()
                .l()
                .unwrap();
            let class_loader = env.new_global_ref(class_loader).unwrap();

            GlobalJVM { jvm, class_loader }
        };
    }
}
