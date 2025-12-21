fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = "../../proto/infrasim.proto";
    let proto_dir = "../../proto";
    
    // Create output directory
    std::fs::create_dir_all("src/generated")?;
    
    // Check if proto file exists
    if std::path::Path::new(proto_file).exists() {
        println!("cargo:rerun-if-changed={}", proto_file);
        
        tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .out_dir("src/generated")
            .compile(&[proto_file], &[proto_dir])?;
    } else {
        // Try alternative path
        let alt_proto_file = "proto/infrasim.proto";
        if std::path::Path::new(alt_proto_file).exists() {
            println!("cargo:rerun-if-changed={}", alt_proto_file);
            
            tonic_build::configure()
                .build_server(true)
                .build_client(true)
                .out_dir("src/generated")
                .compile(&[alt_proto_file], &["proto"])?;
        } else {
            // Create empty generated file if proto doesn't exist
            let generated_path = std::path::Path::new("src/generated/infrasim.rs");
            if !generated_path.exists() {
                std::fs::write(generated_path, "// Proto file not found during build\n")?;
            }
        }
    }
    
    Ok(())
}
