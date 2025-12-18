fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_path = "../../proto/infrasim.proto";
    
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
