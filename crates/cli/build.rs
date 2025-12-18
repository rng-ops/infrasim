fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = "../../proto/infrasim.proto";
    let proto_dir = "../../proto";
    
    std::fs::create_dir_all("src/generated")?;

    if std::path::Path::new(proto_file).exists() {
        println!("cargo:rerun-if-changed={}", proto_file);

        tonic_build::configure()
            .build_server(false)
            .build_client(true)
            .out_dir("src/generated")
            .compile(&[proto_file], &[proto_dir])?;
    }

    Ok(())
}
