/*
SPDX-License-Identifier: GPL-3.0-only

Copyright (C) 2024  Attilio Donà attilio.dona@gmail.com
Copyright (C) 2024  Claudio Carraro carraro.claudio@gmail.com
*/

//! # Opifex
//!
//! [Opifex][wiki] is a Latin word meaning *artisan* or *manufacturer* and referring to
//! a **worker** who created something. This crate defines a [`Worker<Mode>`] struct
//! that, like the web worker interface, represents a task which can communicate
//! back to its creator and that is able to receive messages from it.
//!
//! # Quick Start
//!
//! `Opifex` usage is very simple and intuitive.
//!
//! Suppose we need to implement a simple task: adds two integer numbers, let
//! say `a` and `b`. We can create a simple struct:
//!
//!```rust
//! #[derive(Clone, Debug)]
//! pub struct Sum {
//!     a: i32,
//!     b: i32,
//! }
//!```
//!    
//! This struct will be the message we'll send to the following Adder task:
//!    
//!```rust
//! pub struct Adder {}
//!
//! impl Task for Adder {
//!     type Handle = handle::Worker<handle::TwoWay<Sum, Result>>;
//!     type Output = usize;
//!     
//!     fn spawn(
//!         &self,
//!         wk_hnd: Self::Handle,
//!     ) -> impl std::future::Future<Output = Self::Output> + Send + 'static {
//!         let (mut rx, hnd) = wk_hnd.receiver();
//!         
//!         async move {
//!             let mut count: usize = 0;
//!             
//!             loop {
//!                 tokio::select! {
//!                     Some(sum) = rx.recv() => {
//!                         count += 1;
//!                         if let Err(e) = hnd.post_message(Result::from(sum)).await {
//!                             println!("Oops! Sending message reports: {e}");
//!                         }
//!                     }
//!                     () = hnd.terminated() => {
//!                         println!("Worker is terminated. Bye from adder task!");
//!                         break;
//!                     }
//!                 }
//!             }
//!             
//!             count
//!         }
//!     }
//! }
//!```
//!
//! Having such a task, and wanting to receive its results we can simply spawn
//! a worker with:
//!
//!```rust
//! let adder_worker = Worker::<TwoWay<Sum, Result>>::spawn(Adder {});
//!```
//!
//! In the same way we can implement a Response task and add it as a subscriber
//! to events generated in the Adder task. To do this we use the function
//! on_message:
//!
//!```rust
//! let response_worker = adder_worker.on_message(Response {});
//!```
//!
//! Now we can send Sum messages to the Adder task with:
//!
//!```rust
//! if let Err(e) = adder_worker.post_message(Sum { a: 24, b: 28 }).await {
//!     eprintln!("Oops! sending a message to adder reports: {e}");
//! }
//!```
//!
//! The full example can be found in examples folder.
//!
//! [wiki]: https://en.wikipedia.org/wiki/Opifex

#![doc(
    html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk.png",
    html_favicon_url = "https://www.rust-lang.org/favicon.ico"
)]

use std::{fmt::Display, future::Future};

pub mod handle;
pub mod worker;

pub use worker::Worker;

// default mpcs channel's buffer capacity
pub const BUFFER_CAPACITY: usize = 1000;

#[derive(Clone, Debug)]
pub struct Error {
    cause: String,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.cause)
    }
}

impl std::error::Error for Error {}

impl Error {
    pub(crate) fn from<E: std::error::Error>(e: &E) -> Self {
        Error {
            cause: format!("{e}"),
        }
    }
}

/// The goal of [`Worker`] is to spawn and communicate to and/or control a `task`.
/// To do so the worker needs to inject in the task a sort of handle that can
/// be used from inside of the task to dialogate with its worker.
///
/// To fullfill this goal, the trait [`Task`] was defined.
///
/// [`Worker`]: Worker<Mode>
pub trait Task {
    type Handle: handle::Handle;
    type Output: Send + 'static;

    fn spawn(&self, wk_hnd: Self::Handle) -> impl Future<Output = Self::Output> + Send + 'static;
}
