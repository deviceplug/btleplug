extern crate rust_bluez;

use rust_bluez::manager::Manager;

fn main() {
    let manager = Manager::new().unwrap();

    let adapters = manager.adapters().unwrap();
    let mut adapter = adapters.into_iter().nth(0).unwrap();

    println!("Adapter: {:#?}", adapter);

    adapter = manager.down(&adapter).unwrap();
    println!("Adapter: {:#?}", adapter);

    adapter = manager.up(&adapter).unwrap();
    println!("Adapter: {:#?}", adapter);
//    println!("adapters: {:#?}", get_adapters().unwrap());

//    unsafe {
//
//        let dev_id = hci_get_route(ptr::null());
//        let dd = hci_open_dev(dev_id);
//        println!("dd {:?}", dd);
//
//        reset(dev_id);
//
//        let own_type: u8 = 0x00;
//        let scan_type: u8 = 0x01;
//        let filter_policy: u8 = 0x00;
//        let interval: u16 = 0x0010;
//        let window: u16 = 0x0010;
//        let filter_dup: u8 = 1;
//        let filter_type: u8 = 0;
//
//        let e1 = hci_le_set_scan_parameters(dd, scan_type, interval, window,
//                                             own_type, filter_policy, 1000);
//
//        if e1 < 0 {
//            let s = CString::new("Failed to set scan parameters").unwrap();
//            perror(s.as_ptr());
//        }
//
//        let e2 = hci_le_set_scan_enable(dd, 1, filter_dup, 1000);
//        if e2 < 0 {
//            let s = CString::new("Failed to enable scan").unwrap();
//            perror(s.as_ptr());
//        }
//
//        print_devices(dd, filter_type);
//    };

}

