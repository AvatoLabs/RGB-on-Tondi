fn main() {
    #[cfg(feature = "grpc")]
    {
        // 使用本地 proto 文件（兼容 Tondi 服务端 v1.22.0 的旧格式）
        // 旧格式：RpcTransactionInput/Output 使用扁平字段，无 oneof 包装
        let local_proto_path = "proto";
        
        let rpc_proto = format!("{}/rpc.proto", local_proto_path);
        let messages_proto = format!("{}/messages.proto", local_proto_path);
        
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
                &[local_proto_path],
            )
            .unwrap_or_else(|e| panic!("Failed to compile proto files: {}. Make sure proto files exist at {}", e, local_proto_path));
    }
}

