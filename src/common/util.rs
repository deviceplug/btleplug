// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.

use crate::{Error, Result, api::ValueNotification};
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;
use tokio::sync::broadcast::Receiver;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};

pub fn notifications_stream_from_broadcast_receiver(
    receiver: Receiver<ValueNotification>,
) -> Pin<Box<dyn Stream<Item = Result<ValueNotification>> + Send>> {
    Box::pin(BroadcastStream::new(receiver).map(|item| {
        item.map_err(|BroadcastStreamRecvError::Lagged(skipped)| Error::Lagged(skipped))
    }))
}

#[cfg(test)]
mod tests {
    use super::notifications_stream_from_broadcast_receiver;
    use crate::Error;
    use crate::api::ValueNotification;
    use futures::stream::StreamExt;
    use tokio::sync::broadcast;
    use uuid::Uuid;

    fn notification(value: u8) -> ValueNotification {
        ValueNotification {
            uuid: Uuid::nil(),
            service_uuid: Uuid::nil(),
            value: vec![value],
        }
    }

    #[tokio::test]
    async fn lagged_receiver_reports_skipped_count_then_resumes() {
        let (sender, receiver) = broadcast::channel(2);
        let stream = notifications_stream_from_broadcast_receiver(receiver);
        for value in 0..5 {
            sender.send(notification(value)).unwrap();
        }
        drop(sender);

        let items: Vec<_> = stream.collect().await;

        assert_eq!(items.len(), 3);
        assert!(matches!(items[0], Err(Error::Lagged(3))));
        let delivered: Vec<u8> = items[1..]
            .iter()
            .map(|item| item.as_ref().unwrap().value[0])
            .collect();
        assert_eq!(delivered, [3, 4]);
    }
}
