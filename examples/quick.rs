/*
SPDX-License-Identifier: GPL-3.0-only

Copyright (C) 2024  Attilio Donà attilio.dona@gmail.com
Copyright (C) 2024  Claudio Carraro carraro.claudio@gmail.com
*/

use std::time::Duration;
use tokio::time::sleep;

use opifex::{
    handle,
    worker::{TwoWay, Worker},
    Task,
};

#[derive(Clone, Debug)]
pub struct Sum {
    a: i32,
    b: i32,
}

#[derive(Clone, Debug)]
pub struct Result {
    sum: i32,
}

impl From<Sum> for Result {
    fn from(value: Sum) -> Self {
        let Sum { a, b } = value;
        Result {
            sum: a.saturating_add(b),
        }
    }
}

pub struct Adder {}

impl Task for Adder {
    type Handle = handle::Worker<handle::TwoWay<Sum, Result>>;
    type Output = usize;

    fn spawn(
        &self,
        wk_hnd: Self::Handle,
    ) -> impl std::future::Future<Output = Self::Output> + Send + 'static {
        let (mut rx, hnd) = wk_hnd.receiver();

        async move {
            let mut count: usize = 0;

            loop {
                tokio::select! {
                    Some(sum) = rx.recv() => {
                        count += 1;
                        if let Err(e) = hnd.post_message(Result::from(sum)).await {
                            println!("Oops! Sending message reports: {e}");
                        }
                    }
                    () = hnd.terminated() => {
                        println!("Worker is terminated. Bye from adder task!");
                        break;
                    }
                }
            }

            count
        }
    }
}

pub struct Response;

impl Task for Response {
    type Handle = handle::Worker<handle::OnEvent<Result>>;
    type Output = usize;

    fn spawn(
        &self,
        wk_hnd: Self::Handle,
    ) -> impl std::future::Future<Output = Self::Output> + Send + 'static {
        let (mut rx, hnd) = wk_hnd.receiver();

        async move {
            let mut count: usize = 0;

            loop {
                tokio::select! {
                    Ok(res) = rx.recv() => {
                        count += 1;
                        let Result { sum } = res;
                        println!("{count} Result is {sum:?}");
                    }
                    () = hnd.terminated() => {
                        println!("Worker is terminated. Bye from result task!");
                        break;
                    }
                }
            }

            count
        }
    }
}

#[tokio::main]
async fn main() {
    let adder_worker = Worker::<TwoWay<Sum, Result>>::spawn(Adder {});
    let response_worker = adder_worker.on_message(Response {});

    if let Err(e) = adder_worker.post_message(Sum { a: 24, b: 28 }).await {
        eprintln!("Oops! sending a message to adder reports: {e}");
    }

    if let Err(e) = adder_worker.post_message(Sum { a: 123, b: 45 }).await {
        eprintln!("Oops! sending a message to adder reports: {e}");
    }

    // put to sleep this thread just to let worker's tasks to do the work!
    sleep(Duration::from_secs(2)).await;

    response_worker.terminate();
    adder_worker.terminate();

    // waiting to let terminations happens...
    sleep(Duration::from_secs(1)).await;
}
