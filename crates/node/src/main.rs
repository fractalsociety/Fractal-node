#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if std::env::var("FRACTAL_BOOTSTRAP").is_ok() {
        fractal_node::run_follower().await
    } else {
        fractal_node::run_dev().await
    }
}
