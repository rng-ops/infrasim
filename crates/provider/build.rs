fn main() -> Result<(), Box<dyn std::error::Error>> {
    let infrasim_proto = "../../proto/infrasim.proto";
    let tfplugin_proto = "../../proto/tfplugin6.proto";
    let proto_dir = "../../proto";
    
    std::fs::create_dir_all("src/generated")?;

    println!("cargo:rerun-if-changed={}", infrasim_proto);
    println!("cargo:rerun-if-changed={}", tfplugin_proto);

    // Build InfraSim proto (client only)
    if std::path::Path::new(infrasim_proto).exists() {
        tonic_build::configure()
            .build_server(false)
            .build_client(true)
            .out_dir("src/generated")
            .compile(&[infrasim_proto], &[proto_dir])?;
    }

    // Build Terraform plugin proto (server only)
    if std::path::Path::new(tfplugin_proto).exists() {
        tonic_build::configure()
            .build_server(true)
            .build_client(false)
            .out_dir("src/generated")
            .compile(&[tfplugin_proto], &[proto_dir])?;
    }

    Ok(())
}
