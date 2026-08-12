use std::net::SocketAddr;

/// 本地 Dev API/MCP 的默认监听地址。仅监听回环地址，避免调试接口暴露到局域网。
pub const DEFAULT_LOCAL_API_ADDR: &str = "127.0.0.1:17321";

/// 解析 `TAURI_SSH_LOCAL_API_ADDR`，用于开发实例与已安装实例并行运行时隔离本地 API。
/// 即使允许覆盖端口，也绝不允许监听非回环地址。
pub fn local_api_addr() -> Result<SocketAddr, String> {
    let configured = std::env::var("TAURI_SSH_LOCAL_API_ADDR")
        .unwrap_or_else(|_| DEFAULT_LOCAL_API_ADDR.to_string());
    parse_local_api_addr(&configured)
}

pub fn parse_local_api_addr(value: &str) -> Result<SocketAddr, String> {
    let address = value.trim().parse::<SocketAddr>().map_err(|error| {
        format!("TAURI_SSH_LOCAL_API_ADDR 必须是回环 Socket 地址（例如 127.0.0.1:17322）：{error}")
    })?;
    if !address.ip().is_loopback() {
        return Err("TAURI_SSH_LOCAL_API_ADDR 只能监听 127.0.0.1 或 ::1".to_string());
    }
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::{parse_local_api_addr, DEFAULT_LOCAL_API_ADDR};

    #[test]
    fn accepts_loopback_port_overrides_only() {
        assert_eq!(
            parse_local_api_addr(DEFAULT_LOCAL_API_ADDR)
                .expect("default local API address should be valid")
                .to_string(),
            DEFAULT_LOCAL_API_ADDR
        );
        assert_eq!(
            parse_local_api_addr("[::1]:17322")
                .expect("IPv6 loopback should be valid")
                .to_string(),
            "[::1]:17322"
        );
        assert!(parse_local_api_addr("0.0.0.0:17322").is_err());
        assert!(parse_local_api_addr("192.168.1.8:17322").is_err());
        assert!(parse_local_api_addr("localhost:17322").is_err());
    }
}
