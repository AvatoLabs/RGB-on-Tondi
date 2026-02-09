fn main() {
    #[cfg(feature = "grpc")]
    {
        let external_proto_path = "../../Tondi/rpc/grpc/core/proto";
        
        let rpc_proto = format!("{}/rpc.proto", external_proto_path);
        let messages_proto = format!("{}/messages.proto", external_proto_path);
        
        if !std::path::Path::new(external_proto_path).exists() {
            panic!(
                "External proto path not found: {}\n\
                Please ensure the Tondi repository is available at ../../Tondi/rpc/grpc/core/proto",
                external_proto_path
            );
        }
        
        if !std::path::Path::new(&rpc_proto).exists() {
            panic!("rpc.proto not found at {}. This file is required for RPC service definition.", rpc_proto);
        }
        if !std::path::Path::new(&messages_proto).exists() {
            panic!("messages.proto not found at {}. This file is required for message definitions.", messages_proto);
        }

        tonic_build::configure()
            .build_server(false)
            .compile_protos(
                &[rpc_proto.as_str(), messages_proto.as_str()],
                &[external_proto_path],
            )
            .unwrap_or_else(|e| panic!("Failed to compile proto files: {}. Make sure proto files exist at {}", e, external_proto_path));
    }
}

