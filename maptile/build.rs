use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=idl/maptile.thrift");
    println!("cargo:rerun-if-changed=volo.yml");

    let idl_path = PathBuf::from("idl/maptile.thrift");

    volo_build::Builder::thrift()
        .add_service(idl_path)
        .write()
        .expect("Failed to generate Thrift code");
}
