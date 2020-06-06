// btleplug Source Code File
//
// Copyright 2020 Nathan Fairhurst. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.

use crate::api::ValueNotification;
use std::sync::{Arc, Mutex};

pub fn invoke_handlers<F: FnMut(ValueNotification) + ?Sized>(
    notification_handlers: &Arc<Mutex<Vec<Box<F>>>>,
    n: &ValueNotification,
) -> () {
    // The handlers inside our vector in a mutex are MutFn,
    // which means calling them will mutate their environment.
    // To do this, we'll take ownership of the handler vector
    // by swapping in an empty vector to the mutex.
    // Then, we call each handler and push it back into the
    // mutex when we're done.
    // Ideally we would do this in a way that does not require
    // any extra vector allocation (and be safe!).
    let mut handlers_guard = notification_handlers.lock().unwrap();
    // Next, get our handler count so we allocate our vector with
    // exactly the right size, so there's only ever one allocation
    let handler_count = handlers_guard.len();

    // Now, use replace to move our new (empty) vector into our mutex,
    // and getting our old vector (full of handlers) back.
    let handlers = std::mem::replace(&mut *handlers_guard, Vec::with_capacity(handler_count));

    // We iterate over our old vector, calling our handler, then
    // push it into our new vector that's within the mutex
    handlers.into_iter().for_each(|mut h| {
        h(n.clone());
        (*handlers_guard).push(h)
    });
}
