fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = "../../proto/infrasim.proto";
    let proto_dir = "../../proto";
    
    // Create output directory
    std::fs::create_dir_all("src/generated")?;
    
    // Check if proto file exists
    if std::path::Path::new(proto_file).exists() {
        tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .out_dir("src/generated")
            .compile(&[proto_file], &[proto_dir])?;
    }
    
    Ok(())
}
