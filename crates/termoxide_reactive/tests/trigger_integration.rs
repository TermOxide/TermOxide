use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use termoxide_reactive::{Effect, Trigger};

#[test]
fn trigger_tracks_and_notifies() {
    let counter = Arc::new(AtomicUsize::new(0));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        tokio::task::LocalSet::new()
            .run_until(async move {
                let _ = any_spawner::Executor::init_tokio();

                let owner = reactive_graph::owner::Owner::new();
                owner.set();

                let trigger = Trigger::new();
                let c = counter.clone();

                let t = trigger.clone();
                let _effect = Effect::new(move |_prev: Option<()>| {
                    // register dependency on the trigger
                    t.track();
                    c.fetch_add(1, Ordering::SeqCst);
                    ()
                });

                // wait for initial effect run
                any_spawner::Executor::tick().await;
                assert_eq!(counter.load(Ordering::SeqCst), 1);

                // notify should re-run the effect
                trigger.notify();
                any_spawner::Executor::tick().await;
                assert_eq!(counter.load(Ordering::SeqCst), 2);

                // owner was set for this thread; no RAII guard to drop
            })
            .await;
    });
}
