use jni::{
    JNIEnv,
    descriptors::Desc,
    errors::Error,
    objects::{JClass, JObject, JThrowable},
};
use std::{
    any::Any,
    convert::TryFrom,
    panic::{UnwindSafe, catch_unwind, resume_unwind},
    sync::MutexGuard,
};

/// Result from [`try_block`]. This object can be chained into
/// [`catch`](TryCatchResult::catch) calls to catch exceptions.
pub struct TryCatchResult<'a: 'b, 'b, T> {
    env: &'b JNIEnv<'a>,
    try_result: Result<Result<T, Error>, Error>,
    catch_result: Option<Result<T, Error>>,
}

/// Attempt to execute a block of JNI code. If the code causes an exception
/// to be thrown, it will be stored in the resulting [`TryCatchResult`] for
/// matching with [`catch`](TryCatchResult::catch).
pub fn try_block<'a: 'b, 'b, T>(
    env: &'b JNIEnv<'a>,
    block: impl FnOnce() -> Result<T, Error>,
) -> TryCatchResult<'a, 'b, T> {
    TryCatchResult {
        env,
        try_result: (|| {
            if env.exception_check()? {
                Err(Error::JavaException)
            } else {
                Ok(block())
            }
        })(),
        catch_result: None,
    }
}

impl<'a: 'b, 'b, T> TryCatchResult<'a, 'b, T> {
    pub fn catch(
        self,
        class: impl Desc<'a, JClass<'a>>,
        block: impl FnOnce(JThrowable<'a>) -> Result<T, Error>,
    ) -> Self {
        match (self.try_result, self.catch_result) {
            (Err(e), _) => Self {
                env: self.env,
                try_result: Err(e),
                catch_result: None,
            },
            (Ok(Ok(r)), _) => Self {
                env: self.env,
                try_result: Ok(Ok(r)),
                catch_result: None,
            },
            (Ok(Err(e)), Some(r)) => Self {
                env: self.env,
                try_result: Ok(Err(e)),
                catch_result: Some(r),
            },
            (Ok(Err(Error::JavaException)), None) => {
                let env = self.env;
                let catch_result = (|| {
                    if env.exception_check()? {
                        let ex = env.exception_occurred()?;
                        let _auto_local = env.auto_local(ex.clone());
                        env.exception_clear()?;
                        if env.is_instance_of(ex, class)? {
                            return block(ex).map(|o| Some(o));
                        }
                        env.throw(ex)?;
                    }
                    Ok(None)
                })()
                .transpose();
                Self {
                    env,
                    try_result: Ok(Err(Error::JavaException)),
                    catch_result,
                }
            }
            (Ok(Err(e)), None) => Self {
                env: self.env,
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
pub struct JPanicException<'a: 'b, 'b> {
    internal: JThrowable<'a>,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> JPanicException<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JThrowable<'a>) -> Result<Self, Error> {
        Ok(Self { internal: obj, env })
    }

    pub fn new(env: &'b JNIEnv<'a>, any: Box<dyn Any + Send + 'static>) -> Result<Self, Error> {
        let msg = if let Some(s) = any.downcast_ref::<&str>() {
            env.new_string(s)?
        } else if let Some(s) = any.downcast_ref::<String>() {
            env.new_string(s)?
        } else {
            JObject::null().into()
        };

        let obj = env.new_object(
            "io/github/gedgygedgy/rust/panic/PanicException",
            "(Ljava/lang/String;)V",
            &[msg.into()],
        )?;
        unsafe { env.set_rust_field(obj, "any", any) }?;
        Self::from_env(env, obj.into())
    }

    pub fn get(&self) -> Result<MutexGuard<Box<dyn Any + Send + 'static>>, Error> {
        unsafe { self.env.get_rust_field(self.internal, "any") }
    }

    pub fn take(&self) -> Result<Box<dyn Any + Send + 'static>, Error> {
        unsafe { self.env.take_rust_field(self.internal, "any") }
    }

    pub fn resume_unwind(&self) -> Result<(), Error> {
        resume_unwind(self.take()?);
    }
}

impl<'a: 'b, 'b> TryFrom<JPanicException<'a, 'b>> for Box<dyn Any + Send + 'static> {
    type Error = Error;

    fn try_from(ex: JPanicException<'a, 'b>) -> Result<Self, Error> {
        ex.take()
    }
}

impl<'a: 'b, 'b> From<JPanicException<'a, 'b>> for JThrowable<'a> {
    fn from(ex: JPanicException<'a, 'b>) -> Self {
        ex.internal
    }
}

impl<'a: 'b, 'b> ::std::ops::Deref for JPanicException<'a, 'b> {
    type Target = JThrowable<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

/// Calls the given closure. If it panics, catch the unwind, wrap it in a
/// `io.github.gedgygedgy.rust.panic.PanicException`, and throw it.
pub fn throw_unwind<'a: 'b, 'b, R>(
    env: &'b JNIEnv<'a>,
    f: impl FnOnce() -> R + UnwindSafe,
) -> Result<R, Result<(), Error>> {
    catch_unwind(f).map_err(|e| {
        let old_ex = if env.exception_check()? {
            let ex = env.exception_occurred()?;
            env.exception_clear()?;
            Some(ex)
        } else {
            None
        };
        let ex = JPanicException::new(env, e)?;

        if let Some(old_ex) = old_ex {
            env.call_method(
                ex.clone(),
                "addSuppressed",
                "(Ljava/lang/Throwable;)V",
                &[old_ex.into()],
            )?;
        }
        let ex: JThrowable = ex.into();
        env.throw(ex)?;
        Ok(())
    })
}

#[cfg(test)]
mod test {
    use jni::{JNIEnv, errors::Error, objects::JThrowable};

    use super::super::test_utils;
    use super::try_block;

    fn test_catch<'a: 'b, 'b>(
        env: &'b JNIEnv<'a>,
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
            .find_class("java/lang/IllegalArgumentException")
            .unwrap();
        if let Some(ex) = old_ex {
            env.throw(ex).unwrap();
        }

        let ex = throw_class.map(|c| {
            let ex: JThrowable = env.new_object(c, "()V", &[]).unwrap().into();
            ex
        });

        try_block(env, || {
            if let Some(t) = ex {
                env.throw(t).unwrap();
            }
            try_result
        })
        .catch(illegal_argument_exception, |caught| {
            assert!(!env.exception_check().unwrap());
            assert!(env.is_same_object(ex.unwrap(), caught).unwrap());
            Ok(1)
        })
        .catch("java/lang/ArrayIndexOutOfBoundsException", |caught| {
            assert!(!env.exception_check().unwrap());
            assert!(env.is_same_object(ex.unwrap(), caught).unwrap());
            if rethrow {
                Err(Error::JavaException)
            } else {
                Ok(2)
            }
        })
        .catch("java/lang/IndexOutOfBoundsException", |caught| {
            assert!(!env.exception_check().unwrap());
            assert!(env.is_same_object(ex.unwrap(), caught).unwrap());
            if rethrow {
                env.throw(caught).unwrap();
                Err(Error::JavaException)
            } else {
                Ok(3)
            }
        })
        .catch("java/lang/StringIndexOutOfBoundsException", |caught| {
            assert!(!env.exception_check().unwrap());
            assert!(env.is_same_object(ex.unwrap(), caught).unwrap());
            Ok(4)
        })
        .result()
    }

    #[test]
    fn test_catch_first() {
        test_utils::JVM_ENV.with(|env| {
            assert_eq!(
                test_catch(
                    &env,
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
        test_utils::JVM_ENV.with(|env| {
            assert_eq!(
                test_catch(
                    &env,
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
        test_utils::JVM_ENV.with(|env| {
            assert_eq!(
                test_catch(
                    &env,
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
        test_utils::JVM_ENV.with(|env| {
            assert_eq!(test_catch(&env, None, Ok(0), false).unwrap(), 0);
            assert!(!env.exception_check().unwrap());
        });
    }

    #[test]
    fn test_catch_none() {
        test_utils::JVM_ENV.with(|env| {
            if let Error::JavaException = test_catch(
                &env,
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
                    env.is_instance_of(ex, "java/lang/SecurityException")
                        .unwrap()
                );
            } else {
                panic!("No JavaException");
            }
        });
    }

    #[test]
    fn test_catch_other() {
        test_utils::JVM_ENV.with(|env| {
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
        test_utils::JVM_ENV.with(|env| {
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
        test_utils::JVM_ENV.with(|env| {
            let ex: JThrowable = env
                .new_object("java/lang/IllegalArgumentException", "()V", &[])
                .unwrap()
                .into();
            env.throw(ex).unwrap();

            if let Error::JavaException = test_catch(&env, None, Ok(0), false).unwrap_err() {
                assert!(env.exception_check().unwrap());
                let actual_ex = env.exception_occurred().unwrap();
                env.exception_clear().unwrap();
                assert!(env.is_same_object(actual_ex, ex).unwrap());
            } else {
                panic!("JavaException not found");
            }
        });
    }

    #[test]
    fn test_catch_rethrow() {
        test_utils::JVM_ENV.with(|env| {
            if let Error::JavaException = test_catch(
                &env,
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
                    env.is_instance_of(ex, "java/lang/StringIndexOutOfBoundsException")
                        .unwrap()
                );
            } else {
                panic!("JavaException not found");
            }
        });
    }

    #[test]
    fn test_catch_bogus_rethrow() {
        test_utils::JVM_ENV.with(|env| {
            if let Error::JavaException = test_catch(
                &env,
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
        test_utils::JVM_ENV.with(|env| {
            use jni::{objects::JString, strings::JavaStr};

            const STATIC_MSG: &'static str = "This is a &'static str";
            let ex = super::JPanicException::new(env, Box::new(STATIC_MSG)).unwrap();

            {
                let any = ex.get().unwrap();
                assert_eq!(*any.downcast_ref::<&str>().unwrap(), STATIC_MSG);
            }

            let msg: JString = env
                .call_method(ex.clone(), "getMessage", "()Ljava/lang/String;", &[])
                .unwrap()
                .l()
                .unwrap()
                .into();
            let str = JavaStr::from_env(env, msg).unwrap();
            assert_eq!(str.to_str().unwrap(), STATIC_MSG);
        });
    }

    #[test]
    fn test_panic_exception_string() {
        test_utils::JVM_ENV.with(|env| {
            use jni::{objects::JString, strings::JavaStr};
            use std::any::Any;

            const STRING_MSG: &'static str = "This is a String";
            let ex = super::JPanicException::new(env, Box::new(STRING_MSG.to_string())).unwrap();

            {
                let any = ex.get().unwrap();
                assert_eq!(*any.downcast_ref::<String>().unwrap(), STRING_MSG);
            }

            let msg: JString = env
                .call_method(ex.clone(), "getMessage", "()Ljava/lang/String;", &[])
                .unwrap()
                .l()
                .unwrap()
                .into();
            let str = JavaStr::from_env(env, msg).unwrap();
            assert_eq!(str.to_str().unwrap(), STRING_MSG);

            let any: Box<dyn Any + Send> = ex.take().unwrap();
            assert_eq!(*any.downcast::<String>().unwrap(), STRING_MSG);
        });
    }

    #[test]
    fn test_panic_exception_other() {
        test_utils::JVM_ENV.with(|env| {
            use jni::objects::JObject;
            use std::{any::Any, convert::TryInto};

            let ex = super::JPanicException::new(env, Box::new(42)).unwrap();

            {
                let any = ex.get().unwrap();
                assert_eq!(*any.downcast_ref::<i32>().unwrap(), 42);
            }

            let msg = env
                .call_method(ex.clone(), "getMessage", "()Ljava/lang/String;", &[])
                .unwrap()
                .l()
                .unwrap();
            assert!(env.is_same_object(msg, JObject::null()).unwrap());

            let any: Box<dyn Any + Send> = ex.try_into().unwrap();
            assert_eq!(*any.downcast::<i32>().unwrap(), 42);
        });
    }

    #[test]
    fn test_throw_unwind_ok() {
        test_utils::JVM_ENV.with(|env| {
            let result = super::throw_unwind(env, || 42).unwrap();
            assert_eq!(result, 42);
            assert!(!env.exception_check().unwrap());
        });
    }

    #[test]
    fn test_throw_unwind_panic() {
        test_utils::JVM_ENV.with(|env| {
            super::throw_unwind(env, || panic!("This is a panic"))
                .unwrap_err()
                .unwrap();
            assert!(env.exception_check().unwrap());
            let ex = env.exception_occurred().unwrap();
            env.exception_clear().unwrap();
            assert!(
                env.is_instance_of(ex, "io/github/gedgygedgy/rust/panic/PanicException")
                    .unwrap()
            );

            let suppressed_list = env
                .call_method(ex, "getSuppressed", "()[Ljava/lang/Throwable;", &[])
                .unwrap()
                .l()
                .unwrap();
            assert_eq!(
                env.get_array_length(suppressed_list.into_raw()).unwrap(),
                0
            );

            let ex = super::JPanicException::from_env(env, ex).unwrap();
            let any = ex.take().unwrap();
            let str = any.downcast::<&str>().unwrap();
            assert_eq!(*str, "This is a panic");
        });
    }

    #[test]
    fn test_throw_unwind_panic_suppress() {
        test_utils::JVM_ENV.with(|env| {
            let old_ex: JThrowable = env
                .new_object("java/lang/Exception", "()V", &[])
                .unwrap()
                .into();
            env.throw(old_ex).unwrap();

            super::throw_unwind(env, || panic!("This is a panic"))
                .unwrap_err()
                .unwrap();
            assert!(env.exception_check().unwrap());
            let ex = env.exception_occurred().unwrap();
            env.exception_clear().unwrap();
            assert!(
                env.is_instance_of(ex, "io/github/gedgygedgy/rust/panic/PanicException")
                    .unwrap()
            );

            let suppressed_list = env
                .call_method(ex, "getSuppressed", "()[Ljava/lang/Throwable;", &[])
                .unwrap()
                .l()
                .unwrap();
            assert_eq!(
                env.get_array_length(suppressed_list.into_raw()).unwrap(),
                1
            );
            let suppressed_ex = env
                .get_object_array_element(suppressed_list.into_raw(), 0)
                .unwrap();
            assert!(env.is_same_object(old_ex, suppressed_ex).unwrap());

            let ex = super::JPanicException::from_env(env, ex).unwrap();
            let any = ex.take().unwrap();
            let str = any.downcast::<&str>().unwrap();
            assert_eq!(*str, "This is a panic");
        });
    }

    #[test]
    #[should_panic(expected = "This is a panic")]
    fn test_panic_exception_resume_unwind() {
        test_utils::JVM_ENV.with(|env| {
            let ex = super::JPanicException::new(env, Box::new("This is a panic")).unwrap();
            ex.resume_unwind().unwrap();
        });
    }
}
