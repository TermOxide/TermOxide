use any_spawner::Executor;
use color_eyre::Result;
use hot_reload_app::Counter;
use termoxide_hot_reload::run_app;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;
    Executor::init_tokio()?;

    let local = tokio::task::LocalSet::new();
    local.run_until(run_app(Counter::default())).await
}
