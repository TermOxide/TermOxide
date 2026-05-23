//! Shared test helpers for the `termoxide_reactive` integration tests.
//!
//! Effects in `reactive_graph` schedule their re-runs through
//! `any_spawner::Executor::spawn_local`, so every test that creates an
//! `Effect` needs a global executor installed and a tokio current-thread
//! runtime to drive it.
//!
//! # Why so many `Rc<RefCell<_>>` captures?
//!
//! Most effect-based tests follow this shape:
//!
//! ```ignore
//! let runs = Rc::new(RefCell::new(0u32));
//! Effect::new({
//!     let runs = runs.clone();
//!     move |_| { *runs.borrow_mut() += 1; }
//! });
//! // ... later ...
//! assert_eq!(*runs.borrow(), 1);
//! ```
//!
//! Three constraints stack to force this shape:
//!
//! 1. **`Effect::new` takes `Fn + 'static`, not `FnMut`.** The effect may
//!    re-run any number of times, so the closure is called through a
//!    shared reference — it cannot mutate its captures directly. Interior
//!    mutability is required to record observations from inside.
//!
//! 2. **Effects run on the local set (`!Send`).** That makes `Rc` the
//!    right reference-counted handle (no atomic overhead) and `RefCell`
//!    the right cell (no locking). `Cell` would work for `Copy` types,
//!    but `RefCell` handles `Vec`, `String`, etc. uniformly.
//!
//! 3. **The test body needs to read the recorded value after the effect
//!    has run.** Moving the value into the closure would leave nothing to
//!    assert against, so we share ownership: one `Rc` stays with the test,
//!    a clone is moved into the closure.
//!
//! The clone is scoped to the closure with a block expression
//! (`Effect::new({ let runs = runs.clone(); move |_| ... })`) so the
//! original binding stays usable by name in the surrounding test —
//! avoiding `runs_c` / `runs_clone` shadows that obscure intent.
//!
//! `Memo` is different: its closure must be `Fn + Send + Sync + 'static`,
//! so memo tests use `Arc<AtomicU32>` instead.

#![allow(dead_code)]

use std::future::Future;
use termoxide_reactive::runtime::Owner;

/// Install the tokio executor for `any_spawner` exactly once per process.
pub fn init_executor() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        any_spawner::Executor::init_tokio().expect("init tokio executor");
    });
}

/// Yield control until effects scheduled via the spawner have run.
///
/// Delegates to [`any_spawner::Executor::tick`], which drains pending
/// local tasks for the current frame.
pub async fn flush_effects() {
    any_spawner::Executor::tick().await;
}

/// Set up the executor and a `LocalSet`, then run `body` inside a fresh
/// reactive [`Owner`] that is dropped on completion.
///
/// This is the standard wrapper for async reactive tests. Tests that
/// need additional nested owners can construct them inside `body`; the
/// outer owner installed here stays active for the duration.
pub async fn run_reactive<Fut>(body: Fut)
where
    Fut: Future<Output = ()>,
{
    init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let owner = Owner::new();
            owner.set();
            body.await;
            drop(owner);
        })
        .await;
}
