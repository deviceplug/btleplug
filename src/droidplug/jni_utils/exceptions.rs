use jni::{
    Env,
    descriptors::Desc,
    errors::Error,
    jni_str, jni_sig,
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
        try_result: (|| {
            if env.exception_check()? {
                Err(Error::JavaException)
            } else {
                Ok(block(env))
            }
        })(),
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
                    if env.exception_check()? {
                        let ex = env.exception_occurred()?;
                        env.exception_clear()?;
                        if env.is_instance_of(&ex, class)? {
                            return block(env, ex).map(|o| Some(o));
                        }
                        env.throw(&ex)?;
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
        Ok(Self {
            internal: obj.into(),
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
pub fn throw_panic(
    env: &mut Env,
    panic: Box<dyn Any + Send>,
) -> Result<(), Error> {
    let old_ex = if env.exception_check()? {
        let ex = env.exception_occurred()?;
        env.exception_clear()?;
        Some(ex)
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
    env.throw(&ex)?;
    Ok(())
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
    use jni::{Env, errors::Error, jni_str, jni_sig, objects::{JObject, JThrowable}, strings::JNIString};

    use super::super::test_utils;
    use super::try_block;

    fn test_catch(
        env: &mut Env,
        throw_class: Option<&str>,
        try_result: Result<i32, Error>,
        rethrow: bool,
    ) -> Result<i32, Error> {
        let old_ex = if env.exception_check().unwrap() {
            let ex = env.exception_occurred().unwrap();
            env.exception_clear().unwrap();
            Some(ex)
        } else {
            None
        };
        let illegal_argument_exception = env
            .find_class(jni_str!("java/lang/IllegalArgumentException"))
            .unwrap();
        if let Some(ref ex) = old_ex {
            env.throw(ex).unwrap();
        }

        let ex = throw_class.map(|c| {
            let obj = env.new_object(JNIString::from(c), jni_sig!("()V"), &[]).unwrap();
            JThrowable::from(obj)
        });

        try_block(env, |env| {
            if let Some(ref t) = ex {
                env.throw(t).unwrap();
            }
            try_result
        })
        .catch(env, illegal_argument_exception, |env, caught| {
            assert!(!env.exception_check().unwrap());
            assert!(env.is_same_object(&caught, ex.as_ref().unwrap()).unwrap());
            Ok(1)
        })
        .catch(
            env,
            jni_str!("java/lang/ArrayIndexOutOfBoundsException"),
            |env, caught| {
                assert!(!env.exception_check().unwrap());
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
                assert!(!env.exception_check().unwrap());
                assert!(env.is_same_object(&caught, ex.as_ref().unwrap()).unwrap());
                if rethrow {
                    env.throw(&caught).unwrap();
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
                assert!(!env.exception_check().unwrap());
                assert!(env.is_same_object(&caught, ex.as_ref().unwrap()).unwrap());
                Ok(4)
            },
        )
        .result()
    }

    #[test]
    fn test_catch_first() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
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
            assert!(!env.exception_check().unwrap());
        });
    }

    #[test]
    fn test_catch_second() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
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
            assert!(!env.exception_check().unwrap());
        });
    }

    #[test]
    fn test_catch_third() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
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
            assert!(!env.exception_check().unwrap());
        });
    }

    #[test]
    fn test_catch_ok() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            assert_eq!(test_catch(env, None, Ok(0), false).unwrap(), 0);
            assert!(!env.exception_check().unwrap());
        });
    }

    #[test]
    fn test_catch_none() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            if let Error::JavaException = test_catch(
                env,
                Some("java/lang/SecurityException"),
                Err(Error::JavaException),
                false,
            )
            .unwrap_err()
            {
                assert!(env.exception_check().unwrap());
                let ex = env.exception_occurred().unwrap();
                env.exception_clear().unwrap();
                assert!(
                    env.is_instance_of(&ex, jni_str!("java/lang/SecurityException"))
                        .unwrap()
                );
            } else {
                panic!("No JavaException");
            }
        });
    }

    #[test]
    fn test_catch_other() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            if let Error::InvalidCtorReturn =
                test_catch(env, None, Err(Error::InvalidCtorReturn), false).unwrap_err()
            {
                assert!(!env.exception_check().unwrap());
            } else {
                panic!("InvalidCtorReturn not found");
            }
        });
    }

    #[test]
    fn test_catch_bogus_exception() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            if let Error::JavaException =
                test_catch(env, None, Err(Error::JavaException), false).unwrap_err()
            {
                assert!(!env.exception_check().unwrap());
            } else {
                panic!("JavaException not found");
            }
        });
    }

    #[test]
    fn test_catch_prior_exception() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            let ex = JThrowable::from(
                env.new_object(jni_str!("java/lang/IllegalArgumentException"), jni_sig!("()V"), &[])
                    .unwrap(),
            );
            env.throw(&ex).unwrap();

            if let Error::JavaException = test_catch(env, None, Ok(0), false).unwrap_err() {
                assert!(env.exception_check().unwrap());
                let actual_ex = env.exception_occurred().unwrap();
                env.exception_clear().unwrap();
                assert!(env.is_same_object(&actual_ex, &ex).unwrap());
            } else {
                panic!("JavaException not found");
            }
        });
    }

    #[test]
    fn test_catch_rethrow() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            if let Error::JavaException = test_catch(
                env,
                Some("java/lang/StringIndexOutOfBoundsException"),
                Err(Error::JavaException),
                true,
            )
            .unwrap_err()
            {
                assert!(env.exception_check().unwrap());
                let ex = env.exception_occurred().unwrap();
                env.exception_clear().unwrap();
                assert!(
                    env.is_instance_of(&ex, jni_str!("java/lang/StringIndexOutOfBoundsException"))
                        .unwrap()
                );
            } else {
                panic!("JavaException not found");
            }
        });
    }

    #[test]
    fn test_catch_bogus_rethrow() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            if let Error::JavaException = test_catch(
                env,
                Some("java/lang/ArrayIndexOutOfBoundsException"),
                Err(Error::JavaException),
                true,
            )
            .unwrap_err()
            {
                assert!(!env.exception_check().unwrap());
            } else {
                panic!("JavaException not found");
            }
        });
    }

    #[test]
    fn test_panic_exception_static_str() {
        test_utils::JVM_ENV.with(|cell| {
            let mut guard = cell.borrow_mut();
            let env = &mut *guard;
            use jni::objects::JString;

            const STATIC_MSG: &str = "This is a &'static str";
            let ex = super::JPanicException::new(env, Box::new(STATIC_MSG)).unwrap();

            {
                let any = ex.get(env).unwrap();
                assert_eq!(*any.downcast_ref::<&str>().unwrap(), STATIC_MSG);
            }

            let msg: JString = env
                .call_method(&*ex, jni_str!("getMessage"), jni_sig!("()Ljava/lang/String;"), &[])
                .unwrap()
                .l()
                .unwrap()
                .into();
            let str = env.get_string(&msg).unwrap();
            assert_eq!(<String as From<jni::strings::JavaStr>>::from(str), STATIC_MSG);
        });
    }

    #[test]
    fn test_panic_exception_string() {
        test_utils::JVM_ENV.with(|cell| {
            let mut guard = cell.borrow_mut();
            let env = &mut *guard;
            use jni::objects::JString;
            use std::any::Any;

            const STRING_MSG: &str = "This is a String";
            let ex = super::JPanicException::new(env, Box::new(STRING_MSG.to_string())).unwrap();

            {
                let any = ex.get(env).unwrap();
                assert_eq!(*any.downcast_ref::<String>().unwrap(), STRING_MSG);
            }

            let msg: JString = env
                .call_method(&*ex, jni_str!("getMessage"), jni_sig!("()Ljava/lang/String;"), &[])
                .unwrap()
                .l()
                .unwrap()
                .into();
            let str = env.get_string(&msg).unwrap();
            assert_eq!(<String as From<jni::strings::JavaStr>>::from(str), STRING_MSG);

            let any: Box<dyn Any + Send> = ex.take(env).unwrap();
            assert_eq!(*any.downcast::<String>().unwrap(), STRING_MSG);
        });
    }

    #[test]
    fn test_panic_exception_other() {
        test_utils::JVM_ENV.with(|cell| {
            let mut guard = cell.borrow_mut();
            let env = &mut *guard;
            use jni::objects::JObject;
            use std::any::Any;

            let ex = super::JPanicException::new(env, Box::new(42)).unwrap();

            {
                let any = ex.get(env).unwrap();
                assert_eq!(*any.downcast_ref::<i32>().unwrap(), 42);
            }

            let msg = env
                .call_method(&*ex, jni_str!("getMessage"), jni_sig!("()Ljava/lang/String;"), &[])
                .unwrap()
                .l()
                .unwrap();
            assert!(env.is_same_object(&msg, JObject::null()).unwrap());

            let any: Box<dyn Any + Send> = ex.take(env).unwrap();
            assert_eq!(*any.downcast::<i32>().unwrap(), 42);
        });
    }

    #[test]
    fn test_throw_unwind_ok() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            let result = super::throw_unwind(env, || 42).unwrap();
            assert_eq!(result, 42);
            assert!(!env.exception_check().unwrap());
        });
    }

    #[test]
    fn test_throw_unwind_panic() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            super::throw_unwind(env, || panic!("This is a panic"))
                .unwrap_err()
                .unwrap();
            assert!(env.exception_check().unwrap());
            let ex = env.exception_occurred().unwrap();
            env.exception_clear().unwrap();
            assert!(
                env.is_instance_of(&ex, jni_str!("io/github/gedgygedgy/rust/panic/PanicException"))
                    .unwrap()
            );

            let suppressed_list = env
                .call_method(&ex, jni_str!("getSuppressed"), jni_sig!("()[Ljava/lang/Throwable;"), &[])
                .unwrap()
                .l()
                .unwrap();
            let suppressed_array =
                unsafe { jni::objects::JObjectArray::from_raw(suppressed_list.into_raw()) };
            assert_eq!(env.get_array_length(&suppressed_array).unwrap(), 0);

            let ex_throwable = JThrowable::from(JObject::from(ex));
            let ex = super::JPanicException::from_env(ex_throwable);
            let any = ex.take(env).unwrap();
            let str = any.downcast::<&str>().unwrap();
            assert_eq!(*str, "This is a panic");
        });
    }

    #[test]
    fn test_throw_unwind_panic_suppress() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            let old_ex =
                JThrowable::from(env.new_object(jni_str!("java/lang/Exception"), jni_sig!("()V"), &[]).unwrap());
            env.throw(&old_ex).unwrap();

            super::throw_unwind(env, || panic!("This is a panic"))
                .unwrap_err()
                .unwrap();
            assert!(env.exception_check().unwrap());
            let ex = env.exception_occurred().unwrap();
            env.exception_clear().unwrap();
            assert!(
                env.is_instance_of(&ex, jni_str!("io/github/gedgygedgy/rust/panic/PanicException"))
                    .unwrap()
            );

            let suppressed_list = env
                .call_method(&ex, jni_str!("getSuppressed"), jni_sig!("()[Ljava/lang/Throwable;"), &[])
                .unwrap()
                .l()
                .unwrap();
            let suppressed_array =
                unsafe { jni::objects::JObjectArray::from_raw(suppressed_list.into_raw()) };
            assert_eq!(env.get_array_length(&suppressed_array).unwrap(), 1);
            let suppressed_ex = env.get_object_array_element(&suppressed_array, 0).unwrap();
            assert!(env.is_same_object(&old_ex, &suppressed_ex).unwrap());

            let ex_throwable = JThrowable::from(JObject::from(ex));
            let ex = super::JPanicException::from_env(ex_throwable);
            let any = ex.take(env).unwrap();
            let str = any.downcast::<&str>().unwrap();
            assert_eq!(*str, "This is a panic");
        });
    }

    #[test]
    #[should_panic(expected = "This is a panic")]
    fn test_panic_exception_resume_unwind() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            let ex = super::JPanicException::new(env, Box::new("This is a panic")).unwrap();
            ex.resume_unwind(env).unwrap();
        });
    }
}
