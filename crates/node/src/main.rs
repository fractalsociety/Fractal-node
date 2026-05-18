#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if std::env::args().nth(1).as_deref() == Some("print-devnet-validator-keys") {
        print!("{}", fractal_node::devnet_validator_onboarding_report());
        return Ok(());
    }
    let bootstrap = std::env::var("FRACTAL_BOOTSTRAP").unwrap_or_default();
    if bootstrap.trim().is_empty() {
        fractal_node::run_dev().await
    } else {
        fractal_node::run_follower().await
    }
}
