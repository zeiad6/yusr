use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
    let result = match yusr_server::Config::from_env() {
        Ok(cfg) => yusr_server::run(cfg).await,
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        eprintln!("yusr-server: {e}");
        std::process::exit(1);
    }
}
