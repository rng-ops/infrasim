fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_path = "../../proto/infrasim.proto";
    
    // Create output directory
    std::fs::create_dir_all("src/generated")?;
    
    // Check if proto file exists before proceeding
    if !std::path::Path::new(proto_path).exists() {
        // Try alternative path (in case we're in a different working directory)
        let alt_proto_path = "proto/infrasim.proto";
        if std::path::Path::new(alt_proto_path).exists() {
            println!("cargo:rerun-if-changed={}", alt_proto_path);
            
            tonic_build::configure()
                .build_server(false)
                .build_client(true)
                .out_dir("src/generated")
                .compile(&[alt_proto_path], &["proto"])?;
            return Ok(());
        }
        
        // If proto file doesn't exist, create an empty generated file
        println!("cargo:warning=Proto file not found, skipping gRPC generation");
        let generated_path = std::path::Path::new("src/generated/infrasim.rs");
        if !generated_path.exists() {
            std::fs::write(generated_path, "// Proto file not found during build\n")?;
        }
        return Ok(());
    }
    
    // Only rebuild if the proto file changes
    println!("cargo:rerun-if-changed={}", proto_path);
    
    // Generate the client code (we only need the client, not the server)
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .out_dir("src/generated")
        .compile(&[proto_path], &["../../proto"])?;
    
    Ok(())
}
