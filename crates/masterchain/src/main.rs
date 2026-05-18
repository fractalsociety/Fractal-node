#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    fractal_masterchain::run_masterchain_bft().await
}
