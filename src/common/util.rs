// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.

use futures::channel::mpsc::UnboundedSender;

use crate::api::ValueNotification;
use std::sync::{Arc, Mutex};

pub fn send_notification(
    notification_senders: &Arc<Mutex<Vec<UnboundedSender<ValueNotification>>>>,
    n: &ValueNotification,
) {
    let mut senders = notification_senders.lock().unwrap();
    // Remove sender from the list if the other end of the channel has been dropped.
    senders.retain(|sender| sender.unbounded_send(n.clone()).is_ok());
}
