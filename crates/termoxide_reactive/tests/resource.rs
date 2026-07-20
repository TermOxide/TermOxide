//! Behavior tests for `Resource<T>` and `ResourceState`.

mod common;

use std::{cell::Cell, rc::Rc};

use termoxide_reactive::{Resource, ResourceState, runtime::with_owner};

#[test]
fn resource_state_predicates() {
    let loading: ResourceState<i32> = ResourceState::Loading;
    let ready = ResourceState::Ready(7);
    let error: ResourceState<i32> = ResourceState::Error("oops".into());

    assert!(loading.is_loading());
    assert!(!loading.is_ready());
    assert_eq!(loading.value(), None);

    assert!(!ready.is_loading());
    assert!(ready.is_ready());
    assert_eq!(ready.value(), Some(&7));

    assert!(!error.is_loading());
    assert!(!error.is_ready());
    assert_eq!(error.value(), None);
}

#[tokio::test(flavor = "current_thread")]
async fn starts_in_loading_state() {
    common::run_reactive(async {
        let r: Resource<i32> = Resource::new(|| async { 42 });
        assert!(r.state_untracked().is_loading());
        assert_eq!(r.get(), None);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn transitions_to_ready_after_fetcher_resolves() {
    common::run_reactive(async {
        let r: Resource<String> =
            Resource::new(|| async { String::from("Alice") });
        common::flush_effects().await;
        let state = r.state_untracked();
        assert!(state.is_ready(), "state should be Ready, got {:?}", state);
        assert_eq!(r.get(), Some(String::from("Alice")));
        assert_eq!(r.error(), None);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn fallible_resource_records_error_on_err() {
    common::run_reactive(async {
        let r: Resource<i32> = Resource::new_fallible(|| {
            async { Err::<i32, &'static str>("boom") }
        });
        common::flush_effects().await;
        assert_eq!(r.error().as_deref(), Some("boom"));
        assert_eq!(r.get(), None);
        assert!(!r.state_untracked().is_ready());
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn fallible_resource_records_value_on_ok() {
    common::run_reactive(async {
        let r: Resource<i32> =
            Resource::new_fallible(|| async { Ok::<i32, &'static str>(123) });
        common::flush_effects().await;
        assert_eq!(r.get(), Some(123));
        assert_eq!(r.error(), None);
    })
    .await;
}

// --- KNOWN-FAILING (TDD) ------------------------------------------------
//
// Pins down a missing piece of the `Resource` contract: when the owner
// that created a `Resource` is disposed, the fetcher task spawned by
// `Resource::new` should be cancelled (its body dropped mid-await) so no
// further work happens.
#[tokio::test(flavor = "current_thread")]
#[ignore = "TDD: Resource does not cancel its spawned fetcher when its owner \
            is disposed"]
async fn dropping_owner_cancels_pending_fetcher() {
    common::run_reactive(async {
        let body_polled = Rc::new(Cell::new(false));

        // Spawn the fetcher under a short-lived owner: the executor never
        // polls the spawned task before disposal happens.
        with_owner({
            let body_polled = body_polled.clone();
            || {
                let _: Resource<i32> = Resource::new(move || {
                    async move {
                        // Runs as soon as the future is polled for the
                        // first time. With proper cancellation, the task
                        // is aborted before any first poll and this line
                        // never executes.
                        body_polled.set(true);
                        42
                    }
                });
            }
        });

        common::flush_effects().await;
        assert!(
            !body_polled.get(),
            "fetcher body was polled after its owner was disposed — expected \
             the spawned task to be cancelled and the future dropped"
        );
    })
    .await;
}
