use chrono::Local;

/// 获取当前本地时间的 ISO 格式字符串
pub fn now_local() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// 获取当前本地日期字符串
pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now_local_format() {
        let now = now_local();
        // 格式：2026-03-20 14:30:00
        assert_eq!(now.len(), 19);
        assert!(now.contains('-'));
        assert!(now.contains(':'));
    }

    #[test]
    fn test_today_format() {
        let today = today();
        // 格式：2026-03-20
        assert_eq!(today.len(), 10);
    }
}
