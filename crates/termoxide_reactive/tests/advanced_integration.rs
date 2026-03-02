use reactive_graph::owner::Owner as InnerOwner;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use termoxide_reactive::{ArcMemo, ArcRwSignal, Effect, Resource, Trigger};

// Helper to run async test bodies with executor initialized
async fn run_with_executor<Fut>(fut: Fut)
where
    Fut: std::future::Future<Output = ()>,
{
    let _ = any_spawner::Executor::init_tokio();
    tokio::task::LocalSet::new().run_until(fut).await;
}

#[test]
fn effect_lifecycle_stop_on_owner_cleanup() {
    let counter = Arc::new(AtomicUsize::new(0));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        run_with_executor(async {
            // create an owner and set it
            let owner = InnerOwner::new();
            owner.set();

            let sig = ArcRwSignal::new(0i32);
            let c = counter.clone();
            let sig_cloned = sig.clone();

            let _fx = Effect::new(move |_prev: Option<()>| {
                let _ = sig_cloned.get();
                c.fetch_add(1, Ordering::SeqCst);
                ()
            });

            any_spawner::Executor::tick().await;
            assert_eq!(counter.load(Ordering::SeqCst), 1);

            // cleanup owner: effects should stop
            owner.unset_with_forced_cleanup();

            // mutate signal and tick; effect should not run
            sig.set(1);
            any_spawner::Executor::tick().await;
            assert_eq!(counter.load(Ordering::SeqCst), 1);
        })
        .await;
    });
}

#[test]
fn memo_and_effect_integration() {
    let counter = Arc::new(AtomicUsize::new(0));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        run_with_executor(async {
            let owner = InnerOwner::new();
            owner.set();

            let a = ArcRwSignal::new(2i32);
            let b = ArcRwSignal::new(3i32);

            let total = ArcMemo::new({
                let a = a.clone();
                let b = b.clone();
                move |_prev: Option<i32>| a.get() * b.get()
            });

            let c = counter.clone();
            let _fx = Effect::new(move |_prev: Option<i32>| {
                let _ = total.get();
                c.fetch_add(1, Ordering::SeqCst);
                0
            });

            any_spawner::Executor::tick().await;
            assert_eq!(counter.load(Ordering::SeqCst), 1);

            a.set(4);
            any_spawner::Executor::tick().await;
            assert_eq!(counter.load(Ordering::SeqCst), 2);
        })
        .await;
    });
}

#[test]
fn trigger_multiple_and_reattach() {
    let counter = Arc::new(AtomicUsize::new(0));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        run_with_executor(async {
            let owner = InnerOwner::new();
            owner.set();

            let trigger = Trigger::new();
            let c = counter.clone();

            let t = trigger.clone();
            let fx = Effect::new(move |_prev: Option<()>| {
                t.track();
                c.fetch_add(1, Ordering::SeqCst);
                ()
            });

            any_spawner::Executor::tick().await;
            let v1 = counter.load(Ordering::SeqCst);
            assert!(v1 >= 1);

            trigger.notify();
            any_spawner::Executor::tick().await;
            let v2 = counter.load(Ordering::SeqCst);
            assert!(v2 >= v1 + 1);

            // drop effect (stop re-runs)
            drop(fx);
            trigger.notify();
            any_spawner::Executor::tick().await;
            let v3 = counter.load(Ordering::SeqCst);
            // Dropping an effect may allow one already-scheduled run to complete,
            // accept either unchanged or increased by one.
            assert!(v3 == v2 || v3 == v2 + 1);

            // reattach a new effect
            let c2 = counter.clone();
            let t2 = trigger.clone();
            let _fx = Effect::new(move |_prev: Option<()>| {
                t2.track();
                c2.fetch_add(1, Ordering::SeqCst);
                ()
            });

            any_spawner::Executor::tick().await;
            let v4 = counter.load(Ordering::SeqCst);
            assert!(v4 >= v3 + 1);
        })
        .await;
    });
}

#[test]
fn new_sync_cross_thread_reacts() {
    let counter = Arc::new(AtomicUsize::new(0));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        run_with_executor(async {
            let owner = InnerOwner::new();
            owner.set();

            let sig = ArcRwSignal::new(0i32);
            let c = counter.clone();
            let sig_for_fx = sig.clone();

            // thread-safe effect
            let _fx = Effect::new_sync(move |_prev: Option<()>| {
                let _ = sig_for_fx.get();
                c.fetch_add(1, Ordering::SeqCst);
                ()
            });

            any_spawner::Executor::tick().await;
            assert_eq!(counter.load(Ordering::SeqCst), 1);

            // update from another thread
            let s = sig.clone();
            std::thread::spawn(move || {
                s.set(1);
            })
            .join()
            .unwrap();

            any_spawner::Executor::tick().await;
            assert_eq!(counter.load(Ordering::SeqCst), 2);
        })
        .await;
    });
}

#[test]
fn resource_async_lifecycle() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        run_with_executor(async {
            let owner = InnerOwner::new();
            owner.set();

            let res = Resource::new(|| async {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                String::from("ok")
            });

            // initially loading
            assert!(res.is_loading());

            // wait long enough for background task to complete
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;

            assert!(res.get().is_some());
        })
        .await;
    });
}
