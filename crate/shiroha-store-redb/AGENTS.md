<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-02 -->

# shiroha-store-redb

## Purpose

基于 redb 嵌入式数据库的 `Storage` trait 实现。适用于单机生产部署。数据以 JSON 序列化存储在 3 张表中。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/store.rs` | `RedbStorage`：实现 Storage trait，管理 flows/jobs/events 三张表 |
| `src/lib.rs` | 模块导出 |

## For AI Agents

### Working In This Directory

- 表设计：`flows`（str→bytes）、`jobs`（UUID bytes→bytes）、`events`（32B 复合键→bytes）
- events 复合键 = job_id(16B) + event_id(16B)，使同一 Job 事件在 B-tree 中连续
- 错误映射统一使用 `fn s(e: impl Display) -> ShirohaError` 辅助函数
- list 操作目前是全表扫描 + 过滤，数据量大时需优化为范围查询
- 需导入 `redb::ReadableDatabase` 和 `redb::ReadableTable` trait

### Testing Requirements

- `cargo check -p shiroha-store-redb`
- 存储测试需使用临时文件路径，测试后清理

<!-- MANUAL: -->
