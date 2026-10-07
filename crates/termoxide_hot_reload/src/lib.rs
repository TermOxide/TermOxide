use color_eyre::Result;
use termoxide::App;

/// Runs `app` with hot reload.
///
/// For now it only runs `app` as is.
pub async fn run_app<A: App + Clone + 'static>(app: A) -> Result<()> { termoxide::run_with_app(app).await }
