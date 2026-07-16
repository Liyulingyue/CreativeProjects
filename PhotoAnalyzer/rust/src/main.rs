#[tokio::main]
async fn main() {
    let args = photo_analyzer::CliArgs::parse();
    if let Err(error) = photo_analyzer::run_server(&args.host, args.port, !args.no_open).await {
        eprintln!("[photo_analyzer] server error: {error}");
        std::process::exit(1);
    }
}
