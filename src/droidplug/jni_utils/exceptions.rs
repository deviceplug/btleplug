use jni::{
    Env,
    descriptors::Desc,
    errors::Error,
    jni_sig, jni_str,
    objects::{JClass, JObject, JThrowable},
};
use std::{
    any::Any,
    panic::{UnwindSafe, catch_unwind, resume_unwind},
    sync::MutexGuard,
};

/// Result from [`try_block`]. This object can be chained into
/// [`catch`](TryCatchResult::catch) calls to catch exceptions.
pub struct TryCatchResult<T> {
    try_result: Result<Result<T, Error>, Error>,
    catch_result: Option<Result<T, Error>>,
}

/// Attempt to execute a block of JNI code. If the code causes an exception
/// to be thrown, it will be stored in the resulting [`TryCatchResult`] for
/// matching with [`catch`](TryCatchResult::catch).
pub fn try_block<T>(
    env: &mut Env,
    block: impl FnOnce(&mut Env) -> Result<T, Error>,
) -> TryCatchResult<T> {
    TryCatchResult {
        try_result: if env.exception_check() {
            Err(Error::JavaException)
        } else {
            Ok(block(env))
        },
        catch_result: None,
    }
}

impl<T> TryCatchResult<T> {
    pub fn catch<'local>(
        self,
        env: &mut Env<'local>,
        class: impl Desc<'local, JClass<'local>>,
        block: impl FnOnce(&mut Env<'local>, JThrowable<'local>) -> Result<T, Error>,
    ) -> Self {
        match (self.try_result, self.catch_result) {
            (Err(e), _) => Self {
                try_result: Err(e),
                catch_result: None,
            },
            (Ok(Ok(r)), _) => Self {
                try_result: Ok(Ok(r)),
                catch_result: None,
            },
            (Ok(Err(e)), Some(r)) => Self {
                try_result: Ok(Err(e)),
                catch_result: Some(r),
            },
            (Ok(Err(Error::JavaException)), None) => {
                let catch_result = (|| {
                    if env.exception_check()
                        && let Some(ex) = env.exception_occurred()
                    {
                        env.exception_clear();
                        if env.is_instance_of(&ex, class)? {
                            return block(env, ex).map(|o| Some(o));
                        }
                        // Rethrow — throw() returns Err(JavaException) on success
                        match env.throw(&ex) {
                            Err(Error::JavaException) => {}
                            Err(e) => return Err(e),
                            Ok(()) => {}
                        }
                    }
                    Ok(None)
                })()
                .transpose();
                Self {
                    try_result: Ok(Err(Error::JavaException)),
                    catch_result,
                }
            }
            (Ok(Err(e)), None) => Self {
                try_result: Ok(Err(e)),
                catch_result: None,
            },
        }
    }

    pub fn result(self) -> Result<T, Error> {
        match (self.try_result, self.catch_result) {
            (Err(e), _) => Err(e),
            (Ok(Ok(r)), _) => Ok(r),
            (Ok(Err(_)), Some(r)) => r,
            (Ok(Err(e)), None) => Err(e),
        }
    }
}

/// Wrapper for [`JObject`]s that implement
/// `io.github.gedgygedgy.rust.panic.PanicException`.
pub struct JPanicException<'a> {
    internal: JThrowable<'a>,
}

impl<'a> JPanicException<'a> {
    pub fn from_env(obj: JThrowable<'a>) -> Self {
        Self { internal: obj }
    }

    pub fn new(env: &mut Env<'a>, any: Box<dyn Any + Send + 'static>) -> Result<Self, Error> {
        let msg = if let Some(s) = any.downcast_ref::<&str>() {
            env.new_string(s)?.into()
        } else if let Some(s) = any.downcast_ref::<String>() {
            env.new_string(s)?.into()
        } else {
            JObject::null()
        };

        let obj = env.new_object(
            jni_str!("io/github/gedgygedgy/rust/panic/PanicException"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[(&msg).into()],
        )?;
        unsafe { env.set_rust_field(&obj, jni_str!("any"), any) }?;
        let throwable = env.cast_local::<JThrowable>(obj)?;
        Ok(Self {
            internal: throwable,
        })
    }

    pub fn get<'b>(
        &self,
        env: &'b mut Env,
    ) -> Result<MutexGuard<'b, Box<dyn Any + Send + 'static>>, Error> {
        unsafe { env.get_rust_field(&self.internal, jni_str!("any")) }
    }

    pub fn take(&self, env: &mut Env) -> Result<Box<dyn Any + Send + 'static>, Error> {
        unsafe { env.take_rust_field(&self.internal, jni_str!("any")) }
    }

    pub fn resume_unwind(&self, env: &mut Env) -> Result<(), Error> {
        resume_unwind(self.take(env)?);
    }
}

impl<'a> From<JPanicException<'a>> for JThrowable<'a> {
    fn from(ex: JPanicException<'a>) -> Self {
        ex.internal
    }
}

impl<'a> ::std::ops::Deref for JPanicException<'a> {
    type Target = JThrowable<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

/// Wraps a caught panic payload in a
/// `io.github.gedgygedgy.rust.panic.PanicException` and throws it. If a Java
/// exception is already pending, it will be added as a suppressed exception.
pub fn throw_panic(env: &mut Env, panic: Box<dyn Any + Send>) -> Result<(), Error> {
    let old_ex = if env.exception_check() {
        let ex = env.exception_occurred();
        env.exception_clear();
        ex
    } else {
        None
    };
    let ex = JPanicException::new(env, panic)?;

    if let Some(old_ex) = old_ex {
        env.call_method(
            &*ex,
            jni_str!("addSuppressed"),
            jni_sig!("(Ljava/lang/Throwable;)V"),
            &[(&old_ex).into()],
        )?;
    }
    let ex: JThrowable = ex.into();
    // throw() returns Err(JavaException) on success in jni 0.22
    match env.throw(&ex) {
        Err(Error::JavaException) => Ok(()),
        Err(e) => Err(e),
        Ok(()) => Ok(()),
    }
}

/// Calls the given closure. If it panics, catch the unwind, wrap it in a
/// `io.github.gedgygedgy.rust.panic.PanicException`, and throw it.
pub fn throw_unwind<R>(
    env: &mut Env,
    f: impl FnOnce() -> R + UnwindSafe,
) -> Result<R, Result<(), Error>> {
    catch_unwind(f).map_err(|e| throw_panic(env, e))
}

#[cfg(test)]
mod test {
    use jni::{
        Env,
        errors::Error,
        jni_sig, jni_str,
        objects::{JObject, JThrowable},
        strings::JNIString,
    };

    use super::super::test_utils;
    use super::try_block;

    fn test_catch(
        env: &mut Env,
        throw_class: Option<&str>,
        try_result: Result<i32, Error>,
        rethrow: bool,
    ) -> Result<i32, Error> {
        let old_ex = if env.exception_check() {
            let ex = env.exception_occurred();
            env.exception_clear();
            ex
        } else {
            None
        };
        let illegal_argument_exception = env
            .find_class(jni_str!("java/lang/IllegalArgumentException"))
            .unwrap();
        if let Some(ref ex) = old_ex {
            let _ = env.throw(ex);
        }

        let ex = throw_class.map(|c| {
            let obj = env
                .new_object(JNIString::from(c), jni_sig!("()V"), &[])
                .unwrap();
            env.cast_local::<JThrowable>(obj).unwrap()
        });

        try_block(env, |env| {
            if let Some(ref t) = ex {
                let _ = env.throw(t);
            }
            try_result
        })
        .catch(env, illegal_argument_exception, |env, caught| {
            assert!(!env.exception_check());
            assert!(env.is_same_object(&caught, ex.as_ref().unwrap()).unwrap());
            Ok(1)
        })
        .catch(
            env,
            jni_str!("java/lang/ArrayIndexOutOfBoundsException"),
            |env, caught| {
                assert!(!env.exception_check());
                assert!(env.is_same_object(&caught, ex.as_ref().unwrap()).unwrap());
                if rethrow {
                    Err(Error::JavaException)
                } else {
                    Ok(2)
                }
            },
        )
        .catch(
            env,
            jni_str!("java/lang/IndexOutOfBoundsException"),
            |env, caught| {
                assert!(!env.exception_check());
                assert!(env.is_same_object(&caught, ex.as_ref().unwrap()).unwrap());
                if rethrow {
                    let _ = env.throw(&caught);
                    Err(Error::JavaException)
                } else {
                    Ok(3)
                }
            },
        )
        .catch(
            env,
            jni_str!("java/lang/StringIndexOutOfBoundsException"),
            |env, caught| {
                assert!(!env.exception_check());
                assert!(env.is_same_object(&caught, ex.as_ref().unwrap()).unwrap());
                Ok(4)
            },
        )
        .result()
    }

    #[test]
    fn test_catch_first() {
        test_utils::with_env(|env| {
            assert_eq!(
                test_catch(
                    env,
                    Some("java/lang/IllegalArgumentException"),
                    Err(Error::JavaException),
                    false,
                )
                .unwrap(),
                1
            );
            assert!(!env.exception_check());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_catch_second() {
        test_utils::with_env(|env| {
            assert_eq!(
                test_catch(
                    env,
                    Some("java/lang/ArrayIndexOutOfBoundsException"),
                    Err(Error::JavaException),
                    false,
                )
                .unwrap(),
                2
            );
            assert!(!env.exception_check());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_catch_third() {
        test_utils::with_env(|env| {
            assert_eq!(
                test_catch(
                    env,
                    Some("java/lang/StringIndexOutOfBoundsException"),
                    Err(Error::JavaException),
                    false,
                )
                .unwrap(),
                3
            );
            assert!(!env.exception_check());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_catch_ok() {
        test_utils::with_env(|env| {
            assert_eq!(test_catch(env, None, Ok(0), false).unwrap(), 0);
            assert!(!env.exception_check());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_catch_none() {
        test_utils::with_env(|env| {
            if let Error::JavaException = test_catch(
                env,
                Some("java/lang/SecurityException"),
                Err(Error::JavaException),
                false,
            )
            .unwrap_err()
            {
                assert!(env.exception_check());
                let ex = env.exception_occurred().unwrap();
                env.exception_clear();
                assert!(
                    env.is_instance_of(&ex, jni_str!("java/lang/SecurityException"))
                        .unwrap()
                );
            } else {
                panic!("No JavaException");
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_catch_other() {
        test_utils::with_env(|env| {
            if let Error::InvalidCtorReturn =
                test_catch(env, None, Err(Error::InvalidCtorReturn), false).unwrap_err()
            {
                assert!(!env.exception_check());
            } else {
                panic!("InvalidCtorReturn not found");
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_catch_bogus_exception() {
        test_utils::with_env(|env| {
            if let Error::JavaException =
                test_catch(env, None, Err(Error::JavaException), false).unwrap_err()
            {
                assert!(!env.exception_check());
            } else {
                panic!("JavaException not found");
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_catch_prior_exception() {
        test_utils::with_env(|env| {
            let obj = env
                .new_object(
                    jni_str!("java/lang/IllegalArgumentException"),
                    jni_sig!("()V"),
                    &[],
                )
                .unwrap();
            let ex = env.cast_local::<JThrowable>(obj).unwrap();
            let _ = env.throw(&ex);

            if let Error::JavaException = test_catch(env, None, Ok(0), false).unwrap_err() {
                assert!(env.exception_check());
                let actual_ex = env.exception_occurred().unwrap();
                env.exception_clear();
                assert!(env.is_same_object(&actual_ex, &ex).unwrap());
            } else {
                panic!("JavaException not found");
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_catch_rethrow() {
        test_utils::with_env(|env| {
            if let Error::JavaException = test_catch(
                env,
                Some("java/lang/StringIndexOutOfBoundsException"),
                Err(Error::JavaException),
                true,
            )
            .unwrap_err()
            {
                assert!(env.exception_check());
                let ex = env.exception_occurred().unwrap();
                env.exception_clear();
                assert!(
                    env.is_instance_of(&ex, jni_str!("java/lang/StringIndexOutOfBoundsException"))
                        .unwrap()
                );
            } else {
                panic!("JavaException not found");
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_catch_bogus_rethrow() {
        test_utils::with_env(|env| {
            if let Error::JavaException = test_catch(
                env,
                Some("java/lang/ArrayIndexOutOfBoundsException"),
                Err(Error::JavaException),
                true,
            )
            .unwrap_err()
            {
                assert!(!env.exception_check());
            } else {
                panic!("JavaException not found");
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_panic_exception_static_str() {
        test_utils::with_env(|env| {
            use jni::objects::JString;

            const STATIC_MSG: &str = "This is a &'static str";
            let ex = super::JPanicException::new(env, Box::new(STATIC_MSG)).unwrap();

            {
                let any = ex.get(env).unwrap();
                assert_eq!(*any.downcast_ref::<&str>().unwrap(), STATIC_MSG);
            }

            let msg_obj = env
                .call_method(
                    &*ex,
                    jni_str!("getMessage"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )
                .unwrap()
                .l()
                .unwrap();
            let msg = env.cast_local::<JString>(msg_obj).unwrap();
            let chars = msg.mutf8_chars(env).unwrap();
            assert_eq!(String::from(chars), STATIC_MSG);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_panic_exception_string() {
        test_utils::with_env(|env| {
            use jni::objects::JString;
            use std::any::Any;

            const STRING_MSG: &str = "This is a String";
            let ex = super::JPanicException::new(env, Box::new(STRING_MSG.to_string())).unwrap();

            {
                let any = ex.get(env).unwrap();
                assert_eq!(*any.downcast_ref::<String>().unwrap(), STRING_MSG);
            }

            let msg_obj = env
                .call_method(
                    &*ex,
                    jni_str!("getMessage"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )
                .unwrap()
                .l()
                .unwrap();
            let msg = env.cast_local::<JString>(msg_obj).unwrap();
            let chars = msg.mutf8_chars(env).unwrap();
            assert_eq!(String::from(chars), STRING_MSG);

            let any: Box<dyn Any + Send> = ex.take(env).unwrap();
            assert_eq!(*any.downcast::<String>().unwrap(), STRING_MSG);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_panic_exception_other() {
        test_utils::with_env(|env| {
            use jni::objects::JObject;
            use std::any::Any;

            let ex = super::JPanicException::new(env, Box::new(42)).unwrap();

            {
                let any = ex.get(env).unwrap();
                assert_eq!(*any.downcast_ref::<i32>().unwrap(), 42);
            }

            let msg = env
                .call_method(
                    &*ex,
                    jni_str!("getMessage"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )
                .unwrap()
                .l()
                .unwrap();
            assert!(env.is_same_object(&msg, JObject::null()).unwrap());

            let any: Box<dyn Any + Send> = ex.take(env).unwrap();
            assert_eq!(*any.downcast::<i32>().unwrap(), 42);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_throw_unwind_ok() {
        test_utils::with_env(|env| {
            let result = super::throw_unwind(env, || 42).unwrap();
            assert_eq!(result, 42);
            assert!(!env.exception_check());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_throw_unwind_panic() {
        test_utils::with_env(|env| {
            super::throw_unwind(env, || panic!("This is a panic"))
                .unwrap_err()
                .unwrap();
            assert!(env.exception_check());
            let ex = env.exception_occurred().unwrap();
            env.exception_clear();
            assert!(
                env.is_instance_of(
                    &ex,
                    jni_str!("io/github/gedgygedgy/rust/panic/PanicException")
                )
                .unwrap()
            );

            let suppressed_list = env
                .call_method(
                    &ex,
                    jni_str!("getSuppressed"),
                    jni_sig!("()[Ljava/lang/Throwable;"),
                    &[],
                )
                .unwrap()
                .l()
                .unwrap();
            let suppressed_array = unsafe {
                jni::objects::JObjectArray::<JObject>::from_raw(env, suppressed_list.into_raw())
            };
            assert_eq!(suppressed_array.len(env).unwrap(), 0);

            let ex = super::JPanicException::from_env(ex);
            let any = ex.take(env).unwrap();
            let str = any.downcast::<&str>().unwrap();
            assert_eq!(*str, "This is a panic");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_throw_unwind_panic_suppress() {
        test_utils::with_env(|env| {
            let obj = env
                .new_object(jni_str!("java/lang/Exception"), jni_sig!("()V"), &[])
                .unwrap();
            let old_ex = env.cast_local::<JThrowable>(obj).unwrap();
            let _ = env.throw(&old_ex);

            super::throw_unwind(env, || panic!("This is a panic"))
                .unwrap_err()
                .unwrap();
            assert!(env.exception_check());
            let ex = env.exception_occurred().unwrap();
            env.exception_clear();
            assert!(
                env.is_instance_of(
                    &ex,
                    jni_str!("io/github/gedgygedgy/rust/panic/PanicException")
                )
                .unwrap()
            );

            let suppressed_list = env
                .call_method(
                    &ex,
                    jni_str!("getSuppressed"),
                    jni_sig!("()[Ljava/lang/Throwable;"),
                    &[],
                )
                .unwrap()
                .l()
                .unwrap();
            let suppressed_array = unsafe {
                jni::objects::JObjectArray::<JObject>::from_raw(env, suppressed_list.into_raw())
            };
            assert_eq!(suppressed_array.len(env).unwrap(), 1);
            let suppressed_ex = suppressed_array.get_element(env, 0).unwrap();
            assert!(env.is_same_object(&old_ex, &suppressed_ex).unwrap());

            let ex = super::JPanicException::from_env(ex);
            let any = ex.take(env).unwrap();
            let str = any.downcast::<&str>().unwrap();
            assert_eq!(*str, "This is a panic");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    #[should_panic(expected = "This is a panic")]
    fn test_panic_exception_resume_unwind() {
        test_utils::with_env(|env| {
            let ex = super::JPanicException::new(env, Box::new("This is a panic")).unwrap();
            ex.resume_unwind(env).unwrap();
            Ok(())
        })
        .unwrap();
    }
}
