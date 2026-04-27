use jni::{
    Env,
    errors::Result,
    objects::JByteArray,
    sys::{jbyte, jint},
};
use std::slice;

pub fn slice_to_byte_array<'local>(env: &mut Env<'local>, slice: &[u8]) -> Result<JByteArray<'local>> {
    let obj = env.new_byte_array(slice.len() as jint)?;
    let slice = unsafe { &*(slice as *const [u8] as *const [jbyte]) };
    env.set_byte_array_region(&obj, 0, slice)?;
    Ok(obj)
}

pub fn byte_array_to_vec(env: &Env, array: &JByteArray) -> Result<Vec<u8>> {
    let size = env.get_array_length(array)? as usize;
    let mut result = Vec::with_capacity(size);
    unsafe {
        let result_slice = slice::from_raw_parts_mut(result.as_mut_ptr() as *mut jbyte, size);
        env.get_byte_array_region(array, 0, result_slice)?;
        result.set_len(size);
    }
    Ok(result)
}

#[cfg(test)]
mod test {
    use super::super::test_utils;

    #[test]
    fn test_slice_to_byte_array() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            let obj = super::slice_to_byte_array(env, &[1, 2, 3, 4, 5]).unwrap();
            assert_eq!(env.get_array_length(&obj).unwrap(), 5);

            let mut bytes = [0i8; 5];
            env.get_byte_array_region(&obj, 0, &mut bytes).unwrap();
            assert_eq!(bytes, [1, 2, 3, 4, 5]);
        });
    }

    #[test]
    fn test_byte_array_to_vec() {
        test_utils::JVM_ENV.with(|cell| {
            let env = &mut *cell.borrow_mut();
            let obj = env.new_byte_array(5).unwrap();
            env.set_byte_array_region(&obj, 0, &[1, 2, 3, 4, 5])
                .unwrap();

            let vec = super::byte_array_to_vec(env, &obj).unwrap();
            assert_eq!(vec, vec![1, 2, 3, 4, 5]);
        });
    }
}
