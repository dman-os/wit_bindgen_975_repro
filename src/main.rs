#![allow(unused)]

use color_eyre::eyre;
use color_eyre::eyre::{format_err as ferr, Result as Res};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::*;
use utils::*;

mod utils;

mod wit {
    wasmtime::component::bindgen!({
        world: "plug-world",
        async: true
    });

    pub mod logging {

        wasmtime::component::bindgen!({
            world: "imports",
            path: "wit/deps/logging",
        });
    }
}

type PlugId = Arc<str>;

type PlugRegistry = DHashMap<PlugId, Arc<PlugInfo>>;

pub struct PlugManifest {
    name: String,
    version: String,
    module_path: PathBuf,
}

struct PlugInfo {
    id: PlugId,
    manifest: PlugManifest,
    component: wasmtime::component::Component,
}

type InstanceId = u64;

pub struct InstanceInfo {
    start_ts: time::OffsetDateTime,
    plug: Arc<PlugInfo>,
}

struct Instance {
    info: Arc<InstanceInfo>,
    bindings: wit::PlugWorld,
    store: wasmtime::Store<InstanceState>,
}

struct InstanceState {
    table: wasmtime_wasi::ResourceTable,
    wasi_ctx: wasmtime_wasi::WasiCtx,
    http_ctx: wasmtime_wasi_http::WasiHttpCtx,
    host: Host,
}

impl wasmtime_wasi::WasiView for InstanceState {
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable {
        &mut self.table
    }

    fn ctx(&mut self) -> &mut wasmtime_wasi::WasiCtx {
        &mut self.wasi_ctx
    }
}

impl wasmtime_wasi_http::WasiHttpView for InstanceState {
    fn ctx(&mut self) -> &mut wasmtime_wasi_http::WasiHttpCtx {
        &mut self.http_ctx
    }

    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable {
        &mut self.table
    }
}

struct Host {
    instance_info: Arc<InstanceInfo>,
}

#[wasmtime_wasi::async_trait]
impl wit::newsboy::plugface::host::Host for Host {
    async fn log(&mut self, msg: String) -> () {
        info!(plug = ?self.instance_info.plug.manifest.name, "{msg}");
    }
}

impl wit::logging::wasi::logging::logging::Host for Host {
    fn log(
        &mut self,
        level: wit::logging::wasi::logging::logging::Level,
        ctx: String,
        msg: String,
    ) {
        use wit::logging::wasi::logging::logging::Level;
        let id = &self.instance_info.plug.id;
        let name = &self.instance_info.plug.manifest.name;
        let version = &self.instance_info.plug.manifest.version;
        match level {
            Level::Trace => trace!(%name, %version, %id, %ctx, msg),
            Level::Debug => debug!(%name, %version, %id, %ctx, msg),
            Level::Info => info!(%name, %version, %id, %ctx, msg),
            Level::Warn => warn!(%name, %version, %id, %ctx, msg),
            Level::Error => error!(%name, %version, %id, %ctx, msg),
            Level::Critical => error!(%name, %version, %id, %ctx, "CRITICAL: {msg}"),
        }
    }
}

pub struct Rt {
    engine: wasmtime::Engine,
    linker: wasmtime::component::Linker<InstanceState>,
    // cached_components: DHashMap<String, wasmtime::component::Component>,
    instances: DHashMap<InstanceId, Instance>,
    registry: PlugRegistry,
    instance_ctr: std::sync::atomic::AtomicU64,
}

impl Rt {
    pub fn new() -> Res<Self> {
        let engine = wasmtime::Engine::new(
            wasmtime::Config::new()
                .async_support(true)
                .cache_config_load_default()
                .map_err(|err| ferr!("error reading system's wasmtime cache config: {err}"))?,
        )
        .map_err(|err| ferr!("invalid wasmtime engine config: {err}"))?;
        Ok(Self {
            linker: {
                let mut linker = wasmtime::component::Linker::<InstanceState>::new(&engine);

                wasmtime_wasi::add_to_linker_async(&mut linker).map_err(|err| ferr!(err))?;
                wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)
                    .map_err(|err| ferr!(err))?;
                wit::logging::Imports::add_to_linker(&mut linker, |state| &mut state.host)
                    .map_err(|err| ferr!(err))?;
                wit::PlugWorld::add_to_linker(&mut linker, |state| &mut state.host)
                    .map_err(|err| ferr!(err))?;

                linker
            },
            engine,
            instances: default(),
            registry: default(),
            instance_ctr: default(),
            // cached_components: default(),
        })
    }

    pub async fn load_plug(&mut self, manifest: PlugManifest) -> Result<PlugId, RuntimeError> {
        let id = format!("{}!{}", manifest.name, manifest.version);
        let id: Arc<str> = id.into();
        let component =
            wasmtime::component::Component::from_file(&self.engine, &manifest.module_path)
                .map_err(RuntimeError::ComponentLoadErr)?;
        let info = PlugInfo {
            id: id.clone(),
            manifest,
            component,
        };
        let info = Arc::new(info);
        self.registry.insert(id.clone(), info);
        Ok(id)
    }

    pub async fn start_plug(&mut self, plug_id: &str) -> Result<InstanceId, RuntimeError> {
        let Some(plug_info) = self.registry.get(plug_id) else {
            return Err(RuntimeError::PlugNotFound { id: plug_id.into() });
        };

        let instance_info = InstanceInfo {
            start_ts: time::OffsetDateTime::now_utc(),
            plug: plug_info.clone(),
        };
        let instance_info = Arc::new(instance_info);
        let mut store = wasmtime::Store::new(
            &self.engine,
            InstanceState {
                wasi_ctx: wasmtime_wasi::WasiCtxBuilder::new()
                    .allow_ip_name_lookup(true)
                    .inherit_stdout()
                    .inherit_stderr()
                    .inherit_network()
                    .build(),
                http_ctx: wasmtime_wasi_http::WasiHttpCtx::new(),
                table: default(),
                host: Host {
                    instance_info: instance_info.clone(),
                },
            },
        );

        let bindings =
            wit::PlugWorld::instantiate_async(&mut store, &plug_info.component, &self.linker)
                .await
                .map_err(RuntimeError::ComponentInitErr)?;
        let guest = bindings.newsboy_plugface_plug();

        let version = guest
            .call_version(&mut store)
            .await
            .map_err(RuntimeError::ComponentInitErr)?;
        if version.name != plug_info.manifest.name {
            return Err(RuntimeError::PlugInitErr(ferr!(
                "manifest mismatch on name: {} != {}",
                version.name,
                plug_info.manifest.name
            )));
        }
        if version.version != plug_info.manifest.version {
            return Err(RuntimeError::PlugInitErr(ferr!(
                "manifest mismatch on version: {} != {}",
                version.version,
                plug_info.manifest.version
            )));
        }

        let id = self
            .instance_ctr
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.instances.insert(
            id,
            Instance {
                info: instance_info,
                bindings,
                store,
            },
        );

        Ok(id)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("error loading component: {0}")]
    ComponentLoadErr(wasmtime::Error),
    #[error("error instantiating component: {0}")]
    ComponentInitErr(wasmtime::Error),
    #[error("error setting up plug: {0}")]
    PlugInitErr(eyre::Error),
    #[error("no plug under specified id: {id}")]
    PlugNotFound { id: String },
}

#[tokio::main]
async fn main() -> Res<()> {
    crate::utils::setup_tracing()?;

    info!("pwd = {:?}", std::env::current_dir()?);
    let mut rt = Rt::new()?;
    let manifest = PlugManifest {
        name: "comp".into(),
        version: "0.1.0".into(),
        module_path: "../bin/comp.wasm".into(),
    };
    let plug_id = rt.load_plug(manifest).await?;
    let _instance_id = rt.start_plug(&plug_id).await?;
    Ok(())
}
