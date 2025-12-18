fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = "../../proto/infrasim.proto";
    let proto_dir = "../../proto";
    
    if std::path::Path::new(proto_file).exists() {
        std::fs::create_dir_all("src/generated")?;
        
        tonic_build::configure()
            .build_server(true)
            .build_client(false)
            .out_dir("src/generated")
            .compile(&[proto_file], &[proto_dir])?;
    }
    
    Ok(())
}
