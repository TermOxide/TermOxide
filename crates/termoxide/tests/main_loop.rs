mod common;

use std::{rc::Rc, time::Duration};

use common::{Panics, ProbeApp, ScriptedEvents, SharedBackend, key, local, ms, run, run_traced};
use ratatui::layout::Rect;
use termoxide::{AppMethod, AppPanic, INPUT_POLL, LoopError, LoopFailure, MIN_FRAME, TICK_INTERVAL, Trace};
use tokio::time::Instant;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_draws_the_first_frame_immediately() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));
        let start = Instant::now();

        run(&app, &backend, ScriptedEvents::new([(ms(100), key('q'))]))
            .await
            .expect("loop");

        assert_eq!(app.probe.frames.borrow().first().map(|&(at, _)| at), Some(start));
        assert_eq!(app.probe.frame_count(), 1);
        backend.assert_lines(&["count: 0  ", "10x2      "]);
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_draws_nothing_while_idle() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));

        run(&app, &backend, ScriptedEvents::new([(Duration::from_secs(10), key('q'))]))
            .await
            .expect("loop");

        assert_eq!(app.probe.frame_count(), 1, "ten idle seconds must not repaint");
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_repaints_once_when_a_key_writes_a_signal() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));

        run(&app, &backend, ScriptedEvents::new([(ms(50), key('a')), (ms(500), key('q'))]))
            .await
            .expect("loop");

        assert_eq!(app.probe.frame_count(), 2);
        backend.assert_lines(&["count: 1  ", "10x2      "]);
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_repaints_once_for_a_burst_of_keys_in_one_poll() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));
        let burst = std::iter::repeat_n((ms(50), key('a')), 10);

        run(&app, &backend, ScriptedEvents::new(burst.chain([(ms(500), key('q'))])))
            .await
            .expect("loop");

        assert_eq!(app.probe.frame_count(), 2);
        backend.assert_lines(&["count: 10 ", "10x2      "]);
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_caps_the_frame_rate_under_constant_writes() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));
        let writer = tokio::task::spawn_local({
            let count = app.count;
            async move {
                for _ in 0..160 {
                    tokio::time::sleep(ms(1)).await;
                    count.update(|count| *count += 1);
                }
            }
        });

        run(&app, &backend, ScriptedEvents::new([(ms(500), key('q'))]))
            .await
            .expect("loop");
        writer.await.expect("writer task");

        let frames: Vec<Instant> = app.probe.frames.borrow().iter().map(|&(at, _)| at).collect();
        assert!(
            frames.windows(2).all(|pair| pair[1] - pair[0] >= MIN_FRAME),
            "two frames closer than MIN_FRAME: {frames:?}"
        );
        // 160 ms of writes fit at most 160 / 16 = 10 budgets after the
        // first frame, plus one trailing frame for writes the last budget
        // held back.
        let most = (160 / MIN_FRAME.as_millis() + 2) as usize;
        assert!(
            (2..=most).contains(&frames.len()),
            "{} frames, expected 2..={most}",
            frames.len()
        );
        backend.assert_lines(&["count: 160", "10x2      "]);
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_ticks_at_start_and_every_tick_interval() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));
        let start = Instant::now();
        let running = tokio::task::spawn_local({
            let (app, backend) = (app.clone(), backend.clone());
            async move { run(&app, &backend, ScriptedEvents::new([(Duration::from_secs(1), key('q'))])).await }
        });

        // Between two ticks, so the order of same-instant timers doesn't matter.
        tokio::time::sleep_until(start + ms(950)).await;
        assert_eq!(app.probe.tick_count(), 10, "ticks at 0, 100, ..., 900 ms");

        running.await.expect("loop task").expect("loop");
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_handles_a_key_within_one_input_poll() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));
        let start = Instant::now();

        run(&app, &backend, ScriptedEvents::new([(ms(50), key('a')), (ms(500), key('q'))]))
            .await
            .expect("loop");

        let handled_at = app.probe.events.borrow().first().map(|&(at, _)| at - start);
        assert!(
            handled_at.is_some_and(|latency| latency <= ms(50) + INPUT_POLL),
            "key sent at 50 ms handled at {handled_at:?}"
        );
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_repaints_a_resize_within_one_tick() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));
        let start = Instant::now();
        let running = tokio::task::spawn_local({
            let (app, backend) = (app.clone(), backend.clone());
            async move { run(&app, &backend, ScriptedEvents::new([(ms(500), key('q'))])).await }
        });

        tokio::time::sleep_until(start + ms(150)).await;
        backend.resize(8, 3);
        running.await.expect("loop task").expect("loop");

        let frames = app.probe.frames.borrow();
        let resized = frames.iter().find(|&&(_, viewport)| viewport == Rect::new(0, 0, 8, 3));
        assert!(
            resized.is_some_and(|&(at, _)| at - start <= ms(150) + TICK_INTERVAL),
            "resized at 150 ms, frames: {frames:?}"
        );
        backend.assert_lines(&["count: 0", "8x3     ", "        "]);
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_stops_on_quit_without_delivering_or_drawing_anything_after_it() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));
        let events =
            ScriptedEvents::new([(ms(50), key('a')), (ms(50), key('q')), (ms(50), key('a')), (ms(100), key('a'))]);
        let teardown = Rc::clone(&events.teardowns);

        run(&app, &backend, events).await.expect("a quit is a clean stop");

        assert_eq!(app.probe.delivered(), [key('a'), key('q')]);
        assert_eq!(app.probe.frame_count(), 1);
        assert_eq!(teardown.get(), 1);
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_stops_on_a_draw_error_and_still_tears_down() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));
        let events = ScriptedEvents::new([(ms(500), key('q'))]);
        let teardown = Rc::clone(&events.teardowns);
        backend.fail_draws();

        let report = run(&app, &backend, events).await.expect_err("the draw failed");

        let error = report.downcast_ref::<LoopError>().expect("a LoopError");
        assert!(matches!(error.failure(), Some(LoopFailure::Render(_))), "{error:?}");
        assert!(error.teardown().is_none());
        assert_eq!(teardown.get(), 1);
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_reports_a_teardown_error_after_a_clean_stop() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));
        let events = ScriptedEvents::new([(ms(50), key('q'))]).failing_teardown();

        let report = run(&app, &backend, events).await.expect_err("the reader failed");

        let error = report.downcast_ref::<LoopError>().expect("a LoopError");
        assert!(error.failure().is_none(), "{error:?}");
        assert!(
            matches!(error.teardown(), Some(termoxide_event::Error::Terminal(_))),
            "{error:?}"
        );
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_reports_a_draw_error_and_a_teardown_error_together() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));
        let events = ScriptedEvents::new([(ms(500), key('q'))]).failing_teardown();
        backend.fail_draws();

        let report = run(&app, &backend, events).await.expect_err("both failed");

        let error = report.downcast_ref::<LoopError>().expect("a LoopError");
        assert!(matches!(error.failure(), Some(LoopFailure::Render(_))), "{error:?}");
        assert!(
            matches!(error.teardown(), Some(termoxide_event::Error::Terminal(_))),
            "{error:?}"
        );
    })
    .await;
}

/// The panic a run ended on, checked against where the probe panicked.
fn expect_panic<'a>(report: &'a color_eyre::Report, app: &ProbeApp, method: AppMethod) -> &'a AppPanic {
    let error = report.downcast_ref::<LoopError>().expect("a LoopError");
    let Some(LoopFailure::Panic(panic)) = error.failure() else {
        panic!("expected a panic, got {error:?}");
    };
    assert_eq!(panic.method(), method);
    assert_eq!(panic.message(), format!("{method} blew up"));

    let site = app.probe.panicked.get().expect("the probe panicked");
    let location = panic.location().expect("a panic location");
    assert_eq!(
        (location.file(), location.line(), location.column()),
        (site.file, site.line, site.column)
    );
    panic
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_stops_on_a_panic_in_handle_event() {
    local(async {
        let app = ProbeApp::panicking(Panics { on_key: Some('x'), ..Panics::default() });
        let backend = SharedBackend::new(10, 2);
        let events = ScriptedEvents::new([
            (ms(50), key('a')),
            (ms(100), key('x')),
            (ms(100), key('a')),
            (ms(200), key('a')),
            (ms(300), key('q')),
        ]);
        let teardown = Rc::clone(&events.teardowns);

        let report = run(&app, &backend, events).await.expect_err("handle_event panicked");

        expect_panic(&report, &app, AppMethod::HandleEvent);
        assert_eq!(teardown.get(), 1);
        assert_eq!(app.probe.delivered(), [key('a'), key('x')]);
        app.probe.assert_nothing_after_the_panic();
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_stops_on_a_panic_in_on_tick() {
    local(async {
        let app = ProbeApp::panicking(Panics { on_tick: Some(3), ..Panics::default() });
        let backend = SharedBackend::new(10, 2);
        let events = ScriptedEvents::new([(ms(250), key('a')), (ms(500), key('q'))]);
        let teardown = Rc::clone(&events.teardowns);

        let report = run(&app, &backend, events).await.expect_err("on_tick panicked");

        expect_panic(&report, &app, AppMethod::OnTick);
        assert_eq!(teardown.get(), 1);
        assert_eq!(app.probe.tick_count(), 2);
        assert!(app.probe.delivered().is_empty());
        app.probe.assert_nothing_after_the_panic();
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_stops_on_a_panic_in_build_view() {
    local(async {
        let app = ProbeApp::panicking(Panics { on_frame: Some(2), ..Panics::default() });
        let backend = SharedBackend::new(10, 2);
        let events = ScriptedEvents::new([(ms(50), key('a')), (ms(300), key('a')), (ms(500), key('q'))]);
        let teardown = Rc::clone(&events.teardowns);

        let report = run(&app, &backend, events).await.expect_err("build_view panicked");

        expect_panic(&report, &app, AppMethod::BuildView);
        assert_eq!(teardown.get(), 1);
        assert_eq!(app.probe.frame_count(), 1);
        assert_eq!(app.probe.delivered(), [key('a')]);
        app.probe.assert_nothing_after_the_panic();
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_never_starts_after_a_panic_in_the_first_track_view() {
    local(async {
        let app = ProbeApp::panicking(Panics { in_first_track: true, ..Panics::default() });
        let backend = SharedBackend::new(10, 2);
        let events = ScriptedEvents::new([(ms(50), key('a')), (ms(500), key('q'))]);
        let teardown = Rc::clone(&events.teardowns);

        let report = run(&app, &backend, events).await.expect_err("track_view panicked");

        expect_panic(&report, &app, AppMethod::TrackView);
        assert_eq!(teardown.get(), 1);
        assert_eq!(app.probe.tick_count(), 0);
        assert_eq!(app.probe.frame_count(), 0);
        assert!(app.probe.delivered().is_empty());
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_reports_a_panic_and_a_teardown_error_together() {
    local(async {
        let app = ProbeApp::panicking(Panics { on_key: Some('x'), ..Panics::default() });
        let backend = SharedBackend::new(10, 2);
        let events = ScriptedEvents::new([(ms(50), key('x')), (ms(500), key('q'))]).failing_teardown();

        let report = run(&app, &backend, events).await.expect_err("both failed");

        expect_panic(&report, &app, AppMethod::HandleEvent);
        let error = report.downcast_ref::<LoopError>().expect("a LoopError");
        assert!(
            matches!(error.teardown(), Some(termoxide_event::Error::Terminal(_))),
            "{error:?}"
        );
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
#[cfg_attr(
    not(debug_assertions),
    ignore = "frames are only reliable in unoptimised builds with symbols"
)]
async fn loop_traces_a_panic_from_the_app_frame_to_the_termoxide_boundary() {
    local(async {
        let app = ProbeApp::panicking(Panics { on_key: Some('x'), ..Panics::default() });
        let backend = SharedBackend::new(10, 2);
        let events = ScriptedEvents::new([(ms(50), key('x')), (ms(500), key('q'))]);

        let report = run_traced(&app, &backend, events, true)
            .await
            .expect_err("handle_event panicked");

        let panic = expect_panic(&report, &app, AppMethod::HandleEvent);
        let Trace::Frames(frames) = panic.trace() else {
            panic!("expected frames, got {:?}", panic.trace());
        };
        // The helper that raised the panic, then the method that called it;
        // PDB symbols name that method `…::impl$N::handle_event`.
        let site = app.probe.panicked.get().expect("the probe panicked");
        let [helper, method, ..] = frames.as_slice() else {
            panic!("expected at least two frames: {frames:#?}");
        };
        assert!(helper.symbol().ends_with("common::probe_panic"), "{helper:?}");
        assert!(method.symbol().ends_with("::handle_event"), "{method:?}");
        assert!(method.file().is_some_and(|file| file.ends_with(site.file)), "{method:?}");
        assert_eq!(method.line(), Some(site.line));
        for frame in frames {
            let symbol = frame.symbol();
            assert!(
                !["std::", "core::", "tokio::", "<std::", "<core::", "<tokio::"]
                    .iter()
                    .any(|prefix| symbol.starts_with(prefix)),
                "{frames:#?}"
            );
        }
        assert!(panic.to_string().ends_with("\n  termoxide: App::handle_event"), "{panic}");
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_leaves_the_trace_out_when_it_is_disabled() {
    local(async {
        let app = ProbeApp::panicking(Panics { on_key: Some('x'), ..Panics::default() });
        let backend = SharedBackend::new(10, 2);
        let events = ScriptedEvents::new([(ms(50), key('x')), (ms(500), key('q'))]);

        let report = run(&app, &backend, events).await.expect_err("handle_event panicked");

        assert_eq!(expect_panic(&report, &app, AppMethod::HandleEvent).trace(), &Trace::Disabled);
    })
    .await;
}
