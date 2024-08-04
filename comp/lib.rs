use sdk::wit::nboy::plug;

sdk::wit::nboy::export!(Plug);

struct Plug;

impl plug::Guest for Plug {
    fn version() -> plug::PlugMeta {
        sdk::wit::logging::info("getting online");
        plug::PlugMeta {
            name: env!("CARGO_PKG_NAME").into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}
