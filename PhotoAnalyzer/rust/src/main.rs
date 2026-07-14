#[tokio::main]
async fn main() {
    if let Err(error) = photo_analyzer::run_server_from_env().await {
        eprintln!("[photo_analyzer] server error: {error}");
        std::process::exit(1);
    }
}
