use dashmap::DashMap;
use jni::{JNIEnv, errors::Result, objects::GlobalRef};
use once_cell::sync::OnceCell;

static CLASSCACHE: OnceCell<DashMap<String, GlobalRef>> = OnceCell::new();

pub fn find_add_class(env: &mut JNIEnv, classname: &str) -> Result<()> {
    let cache = CLASSCACHE.get_or_init(|| DashMap::new());
    let cls = env.find_class(classname)?;
    let global = env.new_global_ref(cls)?;
    cache.insert(classname.to_owned(), global);
    Ok(())
}

pub fn get_class(classname: &str) -> Option<GlobalRef> {
    let cache = CLASSCACHE.get_or_init(|| DashMap::new());
    cache.get(classname).map(|pair| pair.value().clone())
}
