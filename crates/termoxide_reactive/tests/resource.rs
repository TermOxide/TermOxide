//! Behavior tests for `Resource<T>` and `ResourceState`.

mod common;

use termoxide_reactive::runtime::Owner;
use termoxide_reactive::{Resource, ResourceState};

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
    common::init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let owner = Owner::new();
            owner.set();
            let r: Resource<i32> = Resource::new(|| async {
                // Yield once so the fetcher cannot resolve before we observe.
                tokio::task::yield_now().await;
                tokio::task::yield_now().await;
                42
            });
            assert!(r.state_untracked().is_loading());
            assert_eq!(r.get(), None);
            drop(owner);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn transitions_to_ready_after_fetcher_resolves() {
    common::init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let owner = Owner::new();
            owner.set();
            let r: Resource<String> = Resource::new(|| async { String::from("Alice") });
            common::flush_effects().await;
            let state = r.state_untracked();
            assert!(state.is_ready(), "state should be Ready, got {:?}", state);
            assert_eq!(r.get(), Some(String::from("Alice")));
            assert_eq!(r.error(), None);
            drop(owner);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn fallible_resource_records_error_on_err() {
    common::init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let owner = Owner::new();
            owner.set();
            let r: Resource<i32> = Resource::new_fallible(|| async {
                Err::<i32, &'static str>("boom")
            });
            common::flush_effects().await;
            assert_eq!(r.error().as_deref(), Some("boom"));
            assert_eq!(r.get(), None);
            assert!(!r.state_untracked().is_ready());
            drop(owner);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn fallible_resource_records_value_on_ok() {
    common::init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let owner = Owner::new();
            owner.set();
            let r: Resource<i32> = Resource::new_fallible(|| async {
                Ok::<i32, &'static str>(123)
            });
            common::flush_effects().await;
            assert_eq!(r.get(), Some(123));
            assert_eq!(r.error(), None);
            drop(owner);
        })
        .await;
}
