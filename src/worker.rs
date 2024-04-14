/*
SPDX-License-Identifier: GPL-3.0-only

Copyright (C) 2024  Attilio Donà attilio.dona@gmail.com
Copyright (C) 2024  Claudio Carraro carraro.claudio@gmail.com
*/

use tokio::sync::{broadcast, mpsc::channel};
use tokio_util::sync::CancellationToken;

use crate::{handle, Error, Task, BUFFER_CAPACITY};

// here I use a mod just to keep clean and ordered the file :)
mod modes {
    use tokio::sync::{broadcast, mpsc::Sender};

    /// This is the Worker’s mode that lets build a worker that is not able to
    /// communicate with the controlled task.
    pub struct Isolated {}

    /// Worker's mode that has a communication channel between the worker and its
    /// task. In this channel the messages, with type Message, can be send from the
    /// worker to the task using the function [`super::Worker<Mode>::post_message()`].
    pub struct OneWay<Message> {
        // used to send messages toward Task
        pub(super) sender_to_tsk: Sender<Message>,
    }

    /// This mode is used when a bidirectional channel is needed between worker
    /// and its task. These messages can have diffent types.
    pub struct TwoWay<Message, TaskMessage> {
        // used to send messages toward Task
        pub(super) sender_to_tsk: Sender<Message>,
        // used by interested tasks to subscribe to messages sent by this worker
        // controlled task.
        pub(super) broadcast_from_tsk: broadcast::Sender<TaskMessage>,
    }
}

pub use modes::*;

// // // // // // // // // // // // // // // // // // // // // // // // // // //

/// As the Web Workers API, [`Worker`] makes it possible to spawn a new
/// [`Task`] and, depending of used `Mode`, having some functions that lets
/// its user to interact with the spawned task.
///
/// The type parameter `Mode` is used to assign the wanted features to the [`Worker`].
/// Not always we want a worker that can interact with the task that it spawned,
/// excluding the possibility to terminate its activity using the function [`terminate`]
/// that is always available.
///
/// A brief description of available modes can be found in [`crate::worker`].
///
/// A [`Worker`] can be generated only using the function [`spawn`].
///
/// [`Worker`]: Worker<Mode>
/// [`terminate`]: Worker<Mode>::terminate
/// [`spawn`]: Worker<Mode>::spawn

pub struct Worker<Mode> {
    // used to terminate Task
    termination_token: CancellationToken,
    // mode is used to differenziate the Worker's behaviour.
    mode: Mode,
}

impl<Mode> Worker<Mode> {
    /// Terminates this worker and the related task.
    pub fn terminate(self) {
        self.termination_token.cancel();
    }
}

// // // // // // // // // // // // // // // // // // // // // // // // // // //

impl Worker<Isolated> {
    /// Creates an isolated worker that can only terminate the spawned task.
    pub fn spawn<T>(task: T) -> Worker<Isolated>
    where
        T: Task<Handle = handle::Worker<handle::Isolated>>,
        <T as Task>::Output: Send + 'static,
    {
        // This token is used to terminate the worker and its controlled task.
        let token = CancellationToken::new();

        // Worker's handle that will be used by the Task to communicate with
        // this worker and to terminate both.
        let wkh = handle::Worker::isolated(token.clone());

        // The Task is spawned here
        tokio::spawn(task.spawn(wkh));

        Worker {
            termination_token: token,
            mode: Isolated {},
        }
    }
}

// // // // // // // // // // // // // // // // // // // // // // // // // // //

impl<Message> Worker<OneWay<Message>> {
    /// Creates a worker that is able to send messages to its controlled `task`.
    pub fn spawn<T>(task: T) -> Worker<OneWay<Message>>
    where
        T: Task<Handle = handle::Worker<handle::OneWay<Message>>>,
        <T as Task>::Output: Send + 'static,
    {
        // This token is used to terminate the worker and its controlled task.
        let token = CancellationToken::new();

        // the channel used by Worker to communicate with its Task.
        let (send_to_task, recv_from_wk) = channel::<Message>(BUFFER_CAPACITY);

        // Worker's handle that will be used by the Task to communicate with
        // this worker and to terminate both.
        let wkh = handle::Worker::one_way(token.clone(), recv_from_wk);

        // The Task is spawned here
        tokio::spawn(task.spawn(wkh));

        Worker {
            termination_token: token,
            mode: OneWay {
                sender_to_tsk: send_to_task,
            },
        }
    }

    /// Send message `msg` to the spawned task.
    pub async fn post_message(&self, msg: Message) -> Result<(), Error> {
        self.mode
            .sender_to_tsk
            .send(msg)
            .await
            .map_err(|e| Error::from(&e))
    }
}

// // // // // // // // // // // // // // // // // // // // // // // // // // //

impl<Message, TaskMessage: Clone> Worker<TwoWay<Message, TaskMessage>> {
    /// Creates a worker that is able to communicate in a bidirectional way with
    /// the `task` that is spowned. The back channel is a broadcast one so many
    /// subscriber tasks will be able to subscribe, with the function [`Self::on_message()`],
    /// to the events sent by this worker's controlled task.
    pub fn spawn<T>(task: T) -> Worker<TwoWay<Message, TaskMessage>>
    where
        T: Task<Handle = handle::Worker<handle::TwoWay<Message, TaskMessage>>>,
        <T as Task>::Output: Send + 'static,
    {
        // This token is used to terminate the worker and its controlled task.
        let token = CancellationToken::new();

        // the channel used by Worker to communicate with its Task.
        let (send_to_task, recv_from_wk) = channel::<Message>(BUFFER_CAPACITY);

        // the broadcast channel used by the Task to communicate with this Worker.
        let (broadcast_to_wk, _) = broadcast::channel::<TaskMessage>(BUFFER_CAPACITY);

        // Worker's handle that will be used by the Task to communicate with
        // this worker and to terminate both.
        let wkh = handle::Worker::two_way(token.clone(), recv_from_wk, broadcast_to_wk.to_owned());

        // The Task is spawned here
        tokio::spawn(task.spawn(wkh));

        Worker {
            termination_token: token,
            mode: TwoWay {
                sender_to_tsk: send_to_task,
                broadcast_from_tsk: broadcast_to_wk,
            },
        }
    }

    /// Send message `msg` to the spawned task.
    pub async fn post_message(&self, msg: Message) -> Result<(), Error> {
        self.mode
            .sender_to_tsk
            .send(msg)
            .await
            .map_err(|e| Error::from(&e))
    }

    /// Let `task` to subscribe to event messages that will be sent by this
    /// two-way worker's task. Every subscription will receive independently
    /// the sent events. The OnEvent handle is able to `terminate` itself and
    /// the subscriber task, but not the two-way worker or task.
    pub fn on_message<T>(&self, task: T) -> Worker<Isolated>
    where
        T: Task<Handle = handle::Worker<handle::OnEvent<TaskMessage>>>,
        <T as Task>::Output: Send + 'static,
    {
        // This token is used to terminate the worker and its controlled task.
        let token = CancellationToken::new();

        // OnEvent worker's handle that will be used by the Task to receive
        // events sent by this two-way task.
        let wkh = handle::Worker::on_event(token.clone(), self.mode.broadcast_from_tsk.subscribe());

        // The Task is spawned here
        tokio::spawn(task.spawn(wkh));

        Worker {
            termination_token: token,
            mode: Isolated {},
        }
    }
}
