use jni::{Env, errors::Result, objects::JByteArray, sys::jbyte};
use std::slice;

pub fn slice_to_byte_array<'local>(
    env: &mut Env<'local>,
    slice: &[u8],
) -> Result<JByteArray<'local>> {
    let obj = env.new_byte_array(slice.len())?;
    let slice = unsafe { &*(slice as *const [u8] as *const [jbyte]) };
    obj.set_region(env, 0, slice)?;
    Ok(obj)
}

pub fn byte_array_to_vec(env: &Env, array: &JByteArray) -> Result<Vec<u8>> {
    let size = array.len(env)?;
    let mut result = Vec::with_capacity(size);
    unsafe {
        let result_slice = slice::from_raw_parts_mut(result.as_mut_ptr() as *mut jbyte, size);
        array.get_region(env, 0, result_slice)?;
        result.set_len(size);
    }
    Ok(result)
}

#[cfg(test)]
mod test {
    use super::super::test_utils;

    #[test]
    fn test_slice_to_byte_array() {
        test_utils::with_env(|env| {
            let obj = super::slice_to_byte_array(env, &[1, 2, 3, 4, 5]).unwrap();
            assert_eq!(obj.len(env).unwrap(), 5);

            let mut bytes = [0i8; 5];
            obj.get_region(env, 0, &mut bytes).unwrap();
            assert_eq!(bytes, [1, 2, 3, 4, 5]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_byte_array_to_vec() {
        test_utils::with_env(|env| {
            let obj = env.new_byte_array(5).unwrap();
            obj.set_region(env, 0, &[1, 2, 3, 4, 5]).unwrap();

            let vec = super::byte_array_to_vec(env, &obj).unwrap();
            assert_eq!(vec, vec![1, 2, 3, 4, 5]);
            Ok(())
        })
        .unwrap();
    }
}
