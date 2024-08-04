pub mod wit {

    pub mod nboy {
        wit_bindgen::generate!({
            path: "../wit/",
            world: "plug-world",
            pub_export_macro: true,
            default_bindings_module: "sdk::wit::nboy",
        });
        pub use exports::newsboy::plugface::plug;
        pub use newsboy::plugface::host::log;
    }

    pub mod logging {
        wit_bindgen::generate!({
            path: "../wit/deps/logging/",
            world: "imports",
        });
        use wasi::logging::logging::*;
        pub fn trace(msg: &str) {
            log(Level::Trace, "TODO", msg)
        }
        pub fn debug(msg: &str) {
            log(Level::Debug, "TODO", msg)
        }
        pub fn info(msg: &str) {
            log(Level::Info, "TODO", msg)
        }
        pub fn warn(msg: &str) {
            log(Level::Warn, "TODO", msg)
        }
        pub fn error(msg: &str) {
            log(Level::Error, "TODO", msg)
        }
    }

    pub mod http {
        wit_bindgen::generate!({
            path: "../wit/",
            world: "wasi:http/imports",
            generate_all
        });
        pub use wasi::http::outgoing_handler::handle;
        pub use wasi::http::types::*;
    }
}
