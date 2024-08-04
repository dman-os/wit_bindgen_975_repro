# RUST_LOG=trace 

mkdir bin -p 
cd bin
curl -fLO \
  https://github.com/bytecodealliance/wasmtime/releases/download/v23.0.1/wasi_snapshot_preview1.reactor.wasm
cd - 

PACKAGE=comp
cargo b --target wasm32-wasi --package $PACKAGE
wasm-tools component new "target/wasm32-wasi/debug/$PACKAGE.wasm" \
  -o ./bin/$PACKAGE.wasm \
  --adapt wasi_snapshot_preview1=./bin/wasi_snapshot_preview1.reactor.wasm
