/*
SPDX-License-Identifier: GPL-3.0-only

Copyright (C) 2024  Attilio Donà attilio.dona@gmail.com
Copyright (C) 2024  Claudio Carraro carraro.claudio@gmail.com
*/

use tokio::sync::{
    broadcast::{self, error::SendError},
    mpsc::Receiver,
};
use tokio_util::sync::{CancellationToken, WaitForCancellationFuture};

// // // // // // // // // // // // // // // // // // // // // // // // // // //

mod private {
    pub trait Sealed {}
}

pub trait Handle: private::Sealed {}

// // // // // // // // // // // // // // // // // // // // // // // // // // //

// here I use a mod just to keep clean and ordered the file :)
mod modes {
    use tokio::sync::{broadcast, mpsc::Receiver};

    /// Isolated handle mode: in this mode worker and task are isolated, so no
    /// messages can be exchanged.
    pub struct Isolated {}

    /// This mode is used when there is the needs to send messages, of type
    /// Message, from the worker towards the controlled task.
    pub struct OneWay<Message> {
        // used to receive all messages sent from the worker
        pub(super) receiver_from_wk: Receiver<Message>,
    }

    /// This mode is used when there is the needs:
    /// * to send messages, of type InMessage, from the worker towards the
    ///   controlled task,
    /// * to send messages, of type OutMessage, from the task towards the
    ///   controlling worker.
    pub struct TwoWay<InMessage, OutMessage> {
        // used to receive all messages sent from the worker
        pub(super) receiver_from_wk: Receiver<InMessage>,
        // used to send messages to the worker.
        pub(crate) broadcast_from_task: broadcast::Sender<OutMessage>,
    }

    /// This mode is used when there is the needs to send messages, of type
    /// OutMessage, from the task towards the controlling worker.
    pub struct OneWayBack<OutMessage> {
        // used to send messages to the worker.
        pub(crate) broadcast_from_task: broadcast::Sender<OutMessage>,
    }

    /// Mode used to inject in subscriber tasks the receiver handle of the
    /// channel in which events are sent.
    pub struct OnEvent<Event> {
        // used by the event subscriber to receive the events.
        pub(crate) receiver_from_task: broadcast::Receiver<Event>,
    }
}

pub use modes::*;

// // // // // // // // // // // // // // // // // // // // // // // // // // //

///
pub struct Worker<Mode> {
    termination_token: CancellationToken,
    mode: Mode,
}

impl<Mode> Worker<Mode> {
    /// Returns a Future that gets fulfilled when the task or the worker had
    /// been terminated.
    pub fn terminated(&self) -> WaitForCancellationFuture<'_> {
        self.termination_token.cancelled()
    }

    /// Terminates the worker and the related task.
    pub fn terminate(self) {
        self.termination_token.cancel();
    }
}

// // // // // // // // // // // // // // // // // // // // // // // // // // //

impl private::Sealed for Worker<Isolated> {}

impl Handle for Worker<Isolated> {}

impl Worker<Isolated> {
    pub(crate) fn isolated(token: CancellationToken) -> Worker<Isolated> {
        Self {
            termination_token: token,
            mode: Isolated {},
        }
    }
}

// // // // // // // // // // // // // // // // // // // // // // // // // // //

impl<Message> private::Sealed for Worker<OneWay<Message>> {}

impl<Message> Handle for Worker<OneWay<Message>> {}

impl<Message> Worker<OneWay<Message>> {
    pub(crate) fn one_way(
        token: CancellationToken,
        from_wk: Receiver<Message>,
    ) -> Worker<OneWay<Message>> {
        Self {
            termination_token: token,
            mode: OneWay {
                receiver_from_wk: from_wk,
            },
        }
    }

    /// This function splits the handle in a tuple with the message receiver
    /// and an isolated handle that is able to terminate the pair task and
    /// worker.
    pub fn receiver(self) -> (Receiver<Message>, Worker<Isolated>) {
        let Worker {
            termination_token,
            mode,
        } = self;
        let OneWay { receiver_from_wk } = mode;

        (receiver_from_wk, Worker::isolated(termination_token))
    }
}

// // // // // // // // // // // // // // // // // // // // // // // // // // //

impl<InMessage, OutMessage> private::Sealed for Worker<TwoWay<InMessage, OutMessage>> {}

impl<InMessage, OutMessage> Handle for Worker<TwoWay<InMessage, OutMessage>> {}

impl<InMessage, OutMessage> Worker<TwoWay<InMessage, OutMessage>> {
    pub(crate) fn two_way(
        token: CancellationToken,
        from_wk: Receiver<InMessage>,
        to_task: broadcast::Sender<OutMessage>,
    ) -> Worker<TwoWay<InMessage, OutMessage>> {
        Self {
            termination_token: token,
            mode: TwoWay {
                receiver_from_wk: from_wk,
                broadcast_from_task: to_task,
            },
        }
    }

    /// This function splits the handle in a tuple with the message receiver
    /// and a one-way bask handle that is able to terminate the pair task and
    /// worker and is able to send messages to all interested tasks.
    pub fn receiver(self) -> (Receiver<InMessage>, Worker<OneWayBack<OutMessage>>) {
        let Worker {
            termination_token,
            mode,
        } = self;
        let TwoWay {
            receiver_from_wk,
            broadcast_from_task,
        } = mode;

        (
            receiver_from_wk,
            Worker::one_way_back(termination_token, broadcast_from_task),
        )
    }
}

// // // // // // // // // // // // // // // // // // // // // // // // // // //

impl<OutMessage> private::Sealed for Worker<OneWayBack<OutMessage>> {}

impl<OutMessage> Handle for Worker<OneWayBack<OutMessage>> {}

impl<OutMessage> Worker<OneWayBack<OutMessage>> {
    pub(crate) fn one_way_back(
        token: CancellationToken,
        to_task: broadcast::Sender<OutMessage>,
    ) -> Worker<OneWayBack<OutMessage>> {
        Self {
            termination_token: token,
            mode: OneWayBack {
                broadcast_from_task: to_task,
            },
        }
    }

    /// Send message `msg` to the subscriber tasks.
    pub async fn post_message(&self, msg: OutMessage) -> Result<usize, SendError<OutMessage>> {
        self.mode.broadcast_from_task.send(msg)
    }
}

// // // // // // // // // // // // // // // // // // // // // // // // // // //

impl<Event> private::Sealed for Worker<OnEvent<Event>> {}

impl<Event> Handle for Worker<OnEvent<Event>> {}

impl<Event> Worker<OnEvent<Event>> {
    pub(crate) fn on_event(
        token: CancellationToken,
        from_task: broadcast::Receiver<Event>,
    ) -> Worker<OnEvent<Event>> {
        Self {
            termination_token: token,
            mode: OnEvent {
                receiver_from_task: from_task,
            },
        }
    }

    /// This function splits the handle in a tuple with the message receiver
    /// and an isolated handle that is able to terminate the pair task and
    /// worker.
    pub fn receiver(self) -> (broadcast::Receiver<Event>, Worker<Isolated>) {
        let Worker {
            termination_token,
            mode,
        } = self;
        let OnEvent { receiver_from_task } = mode;

        (receiver_from_task, Worker::isolated(termination_token))
    }
}
