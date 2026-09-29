mod common;

use std::{rc::Rc, time::Duration};

use common::{ProbeApp, ScriptedEvents, SharedBackend, key, local, ms, run};
use ratatui::layout::Rect;
use termoxide::{INPUT_POLL, MIN_FRAME, TICK_INTERVAL};
use termoxide_rendering::renderer::RenderError;
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

        let error = run(&app, &backend, events).await.expect_err("the draw failed");

        assert!(error.downcast_ref::<RenderError>().is_some(), "{error:?}");
        assert_eq!(teardown.get(), 1);
    })
    .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn loop_reports_a_teardown_error_after_a_clean_stop() {
    local(async {
        let (app, backend) = (ProbeApp::new(), SharedBackend::new(10, 2));
        let events = ScriptedEvents::new([(ms(50), key('q'))]).failing_teardown();

        let error = run(&app, &backend, events).await.expect_err("the reader failed");

        assert!(error.downcast_ref::<termoxide_event::Error>().is_some(), "{error:?}");
    })
    .await;
}
