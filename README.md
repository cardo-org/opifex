# Opifex

[![Crates.io][crates-badge]][crates-url]

[crates-badge]: https://img.shields.io/crates/v/opifex.svg
[crates-url]: https://crates.io/crates/opifex

[Opifex][wiki] is a Latin word meaning *artisan* or *manufacturer* and referring to
a **worker** who created something. This crate defines a `Worker<Mode>` struct
that, like the web worker interface, represents a task which can communicate
back to its creator and that is able to receive messages from it.

# Quick Start

`Opifex` usage is very simple and intuitive.

Suppose we need to implement a simple task: adds two integer numbers, let
say `a` and `b`. We can create a simple struct:

```rust
#[derive(Clone, Debug)]
pub struct Sum {
    a: i32,
     b: i32,
}
```
    
This struct will be the message we'll send to the following Adder task:
    
```rust
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
```

Having such a task, and wanting to receive its results we can simply spawn
a worker with:

```rust
let adder_worker = Worker::<TwoWay<Sum, Result>>::spawn(Adder {});
```

In the same way we can implement a Response task and add it as a subscriber
to events generated in the Adder task. To do this we use the function
on_message:

```rust
let response_worker = adder_worker.on_message(Response {});
```

Now we can send Sum messages to the Adder task with:

```rust
if let Err(e) = adder_worker.post_message(Sum { a: 24, b: 28 }).await {
    eprintln!("Oops! sending a message to adder reports: {e}");
}
```

The full example can be found in examples folder.

[wiki]: https://en.wikipedia.org/wiki/Opifex
