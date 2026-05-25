use jni::{Env, bind_java_type, errors::Result, sys::jlong};
use uuid::Uuid;

bind_java_type! {
    pub JUuid => java.util.UUID,
    constructors {
        fn with_bits(most_significant_bits: jlong, least_significant_bits: jlong),
    },
    methods {
        fn get_least_significant_bits() -> jlong,
        fn get_most_significant_bits() -> jlong,
    },
}

impl JUuid<'_> {
    pub fn new<'local>(env: &mut Env<'local>, uuid: Uuid) -> Result<JUuid<'local>> {
        let val = uuid.as_u128();
        let least = (val & 0xFFFFFFFFFFFFFFFF) as jlong;
        let most = ((val >> 64) & 0xFFFFFFFFFFFFFFFF) as jlong;
        JUuid::with_bits(env, most, least)
    }
}

impl<'local> JUuid<'local> {
    pub fn as_uuid(&self, env: &mut Env<'local>) -> Result<Uuid> {
        let least = self.get_least_significant_bits(env)? as u64;
        let most = self.get_most_significant_bits(env)? as u64;
        let val = ((most as u128) << 64) | (least as u128);
        Ok(Uuid::from_u128(val))
    }
}

#[cfg(test)]
mod test {
    use super::super::test_utils;
    use super::JUuid;
    use jni::{jni_sig, jni_str, objects::JObject, sys::jlong};
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
        test_utils::with_env(|env| {
            for test in TESTS {
                let most = test.most as jlong;
                let least = test.least as jlong;

                let uuid_obj = JUuid::new(env, Uuid::from_u128(test.uuid)).unwrap();
                let obj: JObject = uuid_obj.into();

                let actual_most = env
                    .call_method(
                        &obj,
                        jni_str!("getMostSignificantBits"),
                        jni_sig!("()J"),
                        &[],
                    )
                    .unwrap()
                    .j()
                    .unwrap();
                let actual_least = env
                    .call_method(
                        &obj,
                        jni_str!("getLeastSignificantBits"),
                        jni_sig!("()J"),
                        &[],
                    )
                    .unwrap()
                    .j()
                    .unwrap();
                assert_eq!(actual_most, most);
                assert_eq!(actual_least, least);
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_uuid_as_uuid() {
        test_utils::with_env(|env| {
            for test in TESTS {
                let most = test.most as jlong;
                let least = test.least as jlong;

                let obj = env
                    .new_object(
                        jni_str!("java/util/UUID"),
                        jni_sig!("(JJ)V"),
                        &[most.into(), least.into()],
                    )
                    .unwrap();
                let uuid_obj = env.cast_local::<JUuid>(obj).unwrap();

                assert_eq!(uuid_obj.as_uuid(env).unwrap(), Uuid::from_u128(test.uuid));
            }
            Ok(())
        })
        .unwrap();
    }
}
