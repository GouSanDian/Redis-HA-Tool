fn main() {
    // 阶段四：gRPC 代码生成
    // 注意：需要安装 protoc
    // 在 Debian/Ubuntu: apt-get install protobuf-compiler
    // 或使用 PROST_NO_PROTOC=1 环境变量
    
    // 暂时注释掉 proto 编译，等环境准备好后启用
    // tonic_build::configure()
    //     .build_server(true)
    //     .build_client(true)
    //     .compile(&["proto/api.proto"], &["proto"])
    //     .unwrap();
    
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=proto/api.proto");
}