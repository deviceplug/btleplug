extern crate dbus;

use dbus::{Connection, BusType, Message, stdintf, arg, MessageItem};
use dbus::arg::Array;
use std::collections::HashMap;
use std::error::Error;

static SERVICE_NAME: &'static str = "org.bluez";

fn get_managed_objects(c: &Connection) ->  Result<Vec<MessageItem>, Box<Error>> {
    let m = Message::new_method_call(SERVICE_NAME, "/", "org.freedesktop.DBus.ObjectManager", "GetManagedObjects")?;
    let r = c.send_with_reply_and_block(m, 1000)?;
    Ok(r.get_items())
}

fn main() {
    let conn = Connection::get_private(BusType::System).unwrap();
    use stdintf::OrgFreedesktopDBusProperties;

//    let adapters: arg::Variant<HashMap<String, arg::Variant<Box<arg::RefArg>>>> =
//        base.get("org.bluez", "Metadata").unwrap();

//    println!("Adapters: {:?}", adapters);

    let objects: Vec<MessageItem> = get_managed_objects(&conn).unwrap();
    let z: &[MessageItem] = objects.get(0).unwrap().inner().unwrap();

    for y in z {
        let (path, interfaces) = y.inner().unwrap();
        let x: &[MessageItem] = interfaces.inner().unwrap();
        for interface in x {
            let (i, _) = interface.inner().unwrap();
            let name: &str = i.inner().unwrap();
            if name == "org.bluez.Adapter1" {
                println!("{:?}", path);
            }
        }
    }

}
