// RemoteConfig — 远程网关运行配置
//
// 默认绑定 127.0.0.1，公网由用户自配反向代理（避免内嵌 frpc/easytier 类二进制
// 被国内杀软误报为木马）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    /// 绑定主机。默认 "127.0.0.1"。仅当用户明确知道含义时改为 "0.0.0.0"。
    pub host: String,
    /// 端口。默认 7800。
    pub port: u16,
    /// 路由前缀。默认 "/v1"。
    pub base_path: String,
    /// 是否要求 Bearer Token。默认 true；false 仅供本地调试。
    pub require_token: bool,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 7800,
            base_path: "/v1".into(),
            require_token: true,
        }
    }
}
