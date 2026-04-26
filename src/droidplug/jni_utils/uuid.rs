use jni::{
    JNIEnv,
    errors::Result,
    objects::{JMethodID, JObject},
    signature::{Primitive, ReturnType},
    sys::jlong,
};
use uuid::Uuid;

/// Wrapper for [`JObject`]s that contain `java.util.UUID`. Provides methods
/// to convert to and from a [`Uuid`].
pub struct JUuid<'a> {
    internal: JObject<'a>,
    get_least_significant_bits: JMethodID,
    get_most_significant_bits: JMethodID,
}

impl<'a> JUuid<'a> {
    pub fn from_env(env: &mut JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.find_class("java/util/UUID")?;
        let get_least_significant_bits =
            env.get_method_id(&class, "getLeastSignificantBits", "()J")?;
        let get_most_significant_bits =
            env.get_method_id(&class, "getMostSignificantBits", "()J")?;
        Ok(Self {
            internal: obj,
            get_least_significant_bits,
            get_most_significant_bits,
        })
    }

    pub fn new(env: &mut JNIEnv<'a>, uuid: Uuid) -> Result<Self> {
        let val = uuid.as_u128();
        let least = (val & 0xFFFFFFFFFFFFFFFF) as jlong;
        let most = ((val >> 64) & 0xFFFFFFFFFFFFFFFF) as jlong;

        let class = env.find_class("java/util/UUID")?;
        let obj = env.new_object(&class, "(JJ)V", &[most.into(), least.into()])?;
        let get_least_significant_bits =
            env.get_method_id(&class, "getLeastSignificantBits", "()J")?;
        let get_most_significant_bits =
            env.get_method_id(&class, "getMostSignificantBits", "()J")?;
        Ok(Self {
            internal: obj,
            get_least_significant_bits,
            get_most_significant_bits,
        })
    }

    pub fn as_uuid(&self, env: &mut JNIEnv<'a>) -> Result<Uuid> {
        let least = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_least_significant_bits,
                ReturnType::Primitive(Primitive::Long),
                &[],
            )
        }?
        .j()? as u64;
        let most = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_most_significant_bits,
                ReturnType::Primitive(Primitive::Long),
                &[],
            )
        }?
        .j()? as u64;
        let val = ((most as u128) << 64) | (least as u128);
        Ok(Uuid::from_u128(val))
    }
}

impl<'a> ::std::ops::Deref for JUuid<'a> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a> From<JUuid<'a>> for JObject<'a> {
    fn from(other: JUuid<'a>) -> JObject<'a> {
        other.internal
    }
}

#[cfg(test)]
mod test {
    use super::super::test_utils;
    use super::JUuid;
    use jni::{objects::JObject, sys::jlong};
    use uuid::Uuid;

    struct UuidTest {
        uuid: u128,
        most: u64,
        least: u64,
    }

    const TESTS: &[UuidTest] = &[
        UuidTest {
            uuid: 0x63f0f617_f589_40d0_98be_90747b1ea55a,
            most: 0x63f0f617_f589_40d0,
            least: 0x98be_90747b1ea55a,
        },
        UuidTest {
            uuid: 0xdea61ec0_51a6_4d97_81e0_d7b77e9c03d4,
            most: 0xdea61ec0_51a6_4d97,
            least: 0x81e0_d7b77e9c03d4,
        },
    ];

    #[test]
    fn test_uuid_new() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            for test in TESTS {
                let most = test.most as jlong;
                let least = test.least as jlong;

                let uuid_obj = JUuid::new(env, Uuid::from_u128(test.uuid)).unwrap();
                let obj: JObject = uuid_obj.into();

                let actual_most = env
                    .call_method(&obj, "getMostSignificantBits", "()J", &[])
                    .unwrap()
                    .j()
                    .unwrap();
                let actual_least = env
                    .call_method(&obj, "getLeastSignificantBits", "()J", &[])
                    .unwrap()
                    .j()
                    .unwrap();
                assert_eq!(actual_most, most);
                assert_eq!(actual_least, least);
            }
        });
    }

    #[test]
    fn test_uuid_as_uuid() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            for test in TESTS {
                let most = test.most as jlong;
                let least = test.least as jlong;

                let obj = env
                    .new_object("java/util/UUID", "(JJ)V", &[most.into(), least.into()])
                    .unwrap();
                let uuid_obj = JUuid::from_env(env, obj).unwrap();

                assert_eq!(uuid_obj.as_uuid(env).unwrap(), Uuid::from_u128(test.uuid));
            }
        });
    }
}
