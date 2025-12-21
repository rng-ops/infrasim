fn main() -> Result<(), Box<dyn std::error::Error>> {
    let infrasim_proto = "../../proto/infrasim.proto";
    let tfplugin_proto = "../../proto/tfplugin6.proto";
    let proto_dir = "../../proto";
    
    std::fs::create_dir_all("src/generated")?;

    // Build InfraSim proto (client only)
    if std::path::Path::new(infrasim_proto).exists() {
        println!("cargo:rerun-if-changed={}", infrasim_proto);
        
        tonic_build::configure()
            .build_server(false)
            .build_client(true)
            .out_dir("src/generated")
            .compile(&[infrasim_proto], &[proto_dir])?;
    } else {
        // Try alternative paths
        let alt_infrasim = "proto/infrasim.proto";
        if std::path::Path::new(alt_infrasim).exists() {
            println!("cargo:rerun-if-changed={}", alt_infrasim);
            
            tonic_build::configure()
                .build_server(false)
                .build_client(true)
                .out_dir("src/generated")
                .compile(&[alt_infrasim], &["proto"])?;
        }
    }

    // Build Terraform plugin proto (server only)
    if std::path::Path::new(tfplugin_proto).exists() {
        println!("cargo:rerun-if-changed={}", tfplugin_proto);
        
        tonic_build::configure()
            .build_server(true)
            .build_client(false)
            .out_dir("src/generated")
            .compile(&[tfplugin_proto], &[proto_dir])?;
    } else {
        // Try alternative path
        let alt_tfplugin = "proto/tfplugin6.proto";
        if std::path::Path::new(alt_tfplugin).exists() {
            println!("cargo:rerun-if-changed={}", alt_tfplugin);
            
            tonic_build::configure()
                .build_server(true)
                .build_client(false)
                .out_dir("src/generated")
                .compile(&[alt_tfplugin], &["proto"])?;
        }
    }

    Ok(())
}
