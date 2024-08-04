use super::*;

pub fn setup_tracing() -> Res<()> {
    color_eyre::install()?;
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    // tracing_log::LogTracer::init()?;
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .try_init()
        .map_err(|err| eyre::eyre!(err))?;

    Ok(())
}

#[inline]
pub fn default<T: Default>() -> T {
    std::default::Default::default()
}

pub type DHashMap<K, V> = dashmap::DashMap<K, V, ahash::random_state::RandomState>;
