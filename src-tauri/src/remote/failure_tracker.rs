// FailureTracker — 滑动窗口式鉴权失败计数器
//
// 防暴力枚举：60 秒窗口内失败 ≥10 次 → 全网关临时返回 429。
// 当前实现是"全局阈值"，不区分 IP/Token。如需按 IP 分桶，后续可改为 DashMap<IpAddr, ...>。

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct FailureTracker {
    failures: Mutex<VecDeque<Instant>>,
    threshold: usize,
    window: Duration,
}

impl Default for FailureTracker {
    fn default() -> Self {
        Self {
            failures: Mutex::new(VecDeque::new()),
            threshold: 10,
            window: Duration::from_secs(60),
        }
    }
}

impl FailureTracker {
    /// 记录一次鉴权失败。Mutex poison 时静默忽略（避免 handler 因此返 500）。
    pub fn record_failure(&self) {
        if let Ok(mut q) = self.failures.lock() {
            let now = Instant::now();
            // 清理窗口外的旧记录
            while let Some(&front) = q.front() {
                if now.duration_since(front) > self.window {
                    q.pop_front();
                } else {
                    break;
                }
            }
            q.push_back(now);
        }
    }

    /// 当前窗口内失败次数是否触达阈值。
    pub fn is_locked(&self) -> bool {
        match self.failures.lock() {
            Ok(q) => q.len() >= self.threshold,
            Err(_) => false,
        }
    }
}
