use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use termoxide_reactive::{ArcRwSignal, Effect};

#[test]
fn effect_runs_and_reacts() {
    let counter = Arc::new(AtomicUsize::new(0));

    // Create a current-thread Tokio runtime and a LocalSet, initialize
    // the any_spawner tokio integration, then run the reactive owner.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        tokio::task::LocalSet::new()
            .run_until(async move {
                let _ = any_spawner::Executor::init_tokio();

                // Create an inner Owner directly so we can await ticks
                let owner = reactive_graph::owner::Owner::new();
                owner.set();

                let name = ArcRwSignal::new(String::from("Alice"));
                let c = counter.clone();

                let name_cloned = name.clone();
                let _effect = Effect::new(move |_prev: Option<()>| {
                    // read `name` to register a dependency
                    let _ = name_cloned.get();
                    c.fetch_add(1, Ordering::SeqCst);
                    ()
                });

                // wait for the effect task to run its first tick
                any_spawner::Executor::tick().await;
                assert_eq!(counter.load(Ordering::SeqCst), 1);

                // trigger update -> effect should run again
                name.set(String::from("Bob"));
                any_spawner::Executor::tick().await;
                assert_eq!(counter.load(Ordering::SeqCst), 2);
                // owner was set for this thread; no RAII guard to drop
            })
            .await;
    });
}
