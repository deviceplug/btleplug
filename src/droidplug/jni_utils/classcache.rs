use dashmap::DashMap;
use jni::{Env, errors::Result, objects::{Global, JClass}, strings::JNIString};
use once_cell::sync::OnceCell;
use std::sync::Arc;

static CLASSCACHE: OnceCell<DashMap<String, Arc<Global<JClass<'static>>>>> = OnceCell::new();

pub fn find_add_class(env: &mut Env, classname: &str) -> Result<()> {
    let cache = CLASSCACHE.get_or_init(|| DashMap::new());
    let jni_name = JNIString::from(classname);
    let cls = env.find_class(&jni_name)?;
    let global = env.new_global_ref(&cls)?;
    cache.insert(classname.to_owned(), Arc::new(global));
    Ok(())
}

pub fn get_class(classname: &str) -> Option<Arc<Global<JClass<'static>>>> {
    let cache = CLASSCACHE.get_or_init(|| DashMap::new());
    cache.get(classname).map(|pair| pair.value().clone())
}
