fn main() {
    #[cfg(feature = "grpc")]
    {
        // Build the current Tondi RPC contract. Older node formats are unsupported.
        // Outputs are plain records; inputs distinguish consuming and read-only references.
        let local_proto_path = "proto";

        let rpc_proto = format!("{}/rpc.proto", local_proto_path);
        let messages_proto = format!("{}/messages.proto", local_proto_path);

        if !std::path::Path::new(&rpc_proto).exists() {
            panic!(
                "rpc.proto not found at {}. This file is required for RPC service definition.",
                rpc_proto
            );
        }
        if !std::path::Path::new(&messages_proto).exists() {
            panic!(
                "messages.proto not found at {}. This file is required for message definitions.",
                messages_proto
            );
        }

        tonic_build::configure()
            .build_server(false)
            .compile_protos(
                &[rpc_proto.as_str(), messages_proto.as_str()],
                &[local_proto_path],
            )
            .unwrap_or_else(|e| {
                panic!(
                    "Failed to compile proto files: {}. Make sure proto files exist at {}",
                    e, local_proto_path
                )
            });
    }
}
