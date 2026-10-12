#![cfg(feature = "mcp")]

use agent_doc::mcp::poll_until_settled;
use agent_doc::mcp::types::PendingResult;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => {}
        }
    }
}

struct MockPending {
    polls_remaining: usize,
    result: String,
}

impl PendingResult<String> for MockPending {
    fn poll_settled(&mut self) -> Option<String> {
        if self.polls_remaining == 0 {
            Some(self.result.clone())
        } else {
            self.polls_remaining -= 1;
            None
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn settled(self: Box<Self>) -> String {
        self.result
    }
}

#[test]
fn test_poll_immediate() {
    let mut pending = MockPending {
        polls_remaining: 0,
        result: "immediate_success".to_string(),
    };

    let yields = Arc::new(AtomicUsize::new(0));
    let yields_clone = Arc::clone(&yields);
    let mut current_time = Duration::from_secs(0);

    let res = block_on(poll_until_settled(
        &mut pending,
        Duration::from_secs(10),
        move || {
            yields_clone.fetch_add(1, Ordering::SeqCst);
            std::future::ready(())
        },
        move || {
            current_time += Duration::from_millis(16);
            current_time
        },
    ));

    assert_eq!(res, Ok("immediate_success".to_string()));
    assert_eq!(yields.load(Ordering::SeqCst), 0);
}

#[test]
fn test_poll_after_several_yields() {
    let mut pending = MockPending {
        polls_remaining: 3,
        result: "delayed_success".to_string(),
    };

    let yields = Arc::new(AtomicUsize::new(0));
    let yields_clone = Arc::clone(&yields);
    let mut current_time = Duration::from_secs(0);

    let res = block_on(poll_until_settled(
        &mut pending,
        Duration::from_secs(10),
        move || {
            yields_clone.fetch_add(1, Ordering::SeqCst);
            std::future::ready(())
        },
        move || {
            current_time += Duration::from_millis(16);
            current_time
        },
    ));

    assert_eq!(res, Ok("delayed_success".to_string()));
    assert_eq!(yields.load(Ordering::SeqCst), 3);
}

#[test]
fn test_poll_timeout() {
    let mut pending = MockPending {
        polls_remaining: 1000,
        result: "never_reached".to_string(),
    };

    let yields = Arc::new(AtomicUsize::new(0));
    let yields_clone = Arc::clone(&yields);
    let mut current_time = Duration::from_secs(0);

    let res = block_on(poll_until_settled(
        &mut pending,
        Duration::from_millis(100),
        move || {
            yields_clone.fetch_add(1, Ordering::SeqCst);
            std::future::ready(())
        },
        move || {
            // Advance by 30ms each poll
            current_time += Duration::from_millis(30);
            current_time
        },
    ));

    assert_eq!(
        res,
        Err("MCP tool execution timed out waiting for the render loop".to_string())
    );
    assert!(yields.load(Ordering::SeqCst) > 0);
}
