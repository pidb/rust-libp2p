use prost_build;

use std::env;
use std::path::Path;
use std::path::PathBuf;

fn main() {
    let manifest_dir =
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR environment variable unset");

    let proto_dir = Path::new(&manifest_dir).join("protos");
    let protos = [&Path::new(&proto_dir).join(Path::new("message.proto"))];

    for pb in protos {
        println!(
            "cargo::rerun-if-changed={}",
            pb.to_str().expect("PathBuf::to_str failed")
        );
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR environment variable unset"));

    prost_build::Config::new()
        .message_attribute(
            "distributedkv.ApiRequest",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "distributedkv.ApiResponse",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .out_dir(out_dir)
        .compile_protos(&protos, &[&proto_dir])
        .unwrap()
}
