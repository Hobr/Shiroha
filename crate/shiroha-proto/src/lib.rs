//! protobuf / gRPC 生成代码入口。
//!
//! 其余 crate 统一从这里引入服务定义，避免直接依赖 `tonic::include_proto!`
//! 展开的模块路径。
pub mod shiroha_api {
    tonic::include_proto!("shiroha.api.v1");
}
