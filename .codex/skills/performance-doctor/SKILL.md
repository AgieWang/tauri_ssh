---
name: performance-doctor
description: |
  用于基于测量诊断 Tauri 应用运行性能、内存、CPU、启动、前端渲染、安装包体积或 Rust 编译性能；Skill/提示词优化和普通代码整理明确不触发。

  触发场景：
  - 应用启动慢、界面卡顿、CPU 异常或内存持续增长
  - 需要采样 React 渲染、Rust 热点、IPC 频率或 I/O 阻塞
  - 需要测量和优化 Tauri 安装包/前端 bundle 体积
  - 需要分析 Cargo 编译时间、链接时间或增量构建退化

  触发词：运行性能、CPU profiling、内存泄漏、启动耗时、React Profiler、cargo build timings、bundle 体积、卡顿采样、性能基线
---

# Tauri 性能诊断

## 边界与排除

只有存在应用运行、资源占用、启动、体积或编译性能目标时使用本技能。以下请求不得触发：Skill 优化、提示词优化、token 优化、工作流提效、代码精简、文档压缩、一般性“优化方案”。

功能错误优先 `bug-detective`；普通安装包生成使用 `tauri-packaging`；仅做代码风格重构使用对应领域技能。

## 强制规则

1. 先定义指标、设备/平台、数据集、操作路径与可接受阈值，再修改代码。
2. 获取可复现基线和 profile/trace；禁止仅凭直觉添加缓存、并发、memo 或 release profile 参数。
3. 一次只验证一个主要假设，保留前后相同条件的数据和功能正确性对照。
4. 不以牺牲数据一致性、安全、错误可见性、可访问性或跨平台行为换取数字改善。
5. 编译和体积优化要评估开发/发布 profile、CI、本地缓存和最终产物的不同影响。
6. 没有可比数据时只能报告诊断发现，不能宣称性能已提升。

## 执行流程

1. 复现并记录基线：耗时、CPU、内存、帧、bundle 或编译阶段。
2. 用最贴近问题的工具定位 Rust、React、IPC、SQLite、网络或构建瓶颈。
3. 排序假设，实施最小、可回滚的改动。
4. 在相同条件重测，并运行功能测试、构建和目标平台验收。
5. 报告原始值、新值、波动、限制和仍未验证的风险。

## 按需参考

需要 Cargo profile、链接器、React/Vite、`cargo bloat` 或具体调优示例时读取 [references/profiling-and-tuning.md](references/profiling-and-tuning.md)。未建立基线前不要直接套用其中配置。

## 完成条件

- 瓶颈有 profile/trace 或可重复测量证据。
- 改动前后数据可比，功能正确性和安全边界未回归。
- 相关测试、构建和目标平台验证通过。
- UTF-8 无 BOM，`git diff --check` 通过。
