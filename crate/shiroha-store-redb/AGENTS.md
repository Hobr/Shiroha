<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-07 -->

# shiroha-store-redb

## Purpose

基于 redb 嵌入式数据库的 `Storage` trait 实现。适用于单机生产部署。数据以 JSON 序列化存储在 flows、flow_versions、wasm_modules、jobs、events、kv 6 张表中。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/store.rs` | `RedbStorage`：实现 Storage / CapabilityStore，管理 flows、flow_versions、wasm_modules、jobs、events、kv，并在打开数据库时执行 flow_versions 键迁移 |
| `src/lib.rs` | 模块导出 |

## For AI Agents

### Working In This Directory

- 表设计：`flows`（latest alias）、`flow_versions`（`hex(flow_id)\0version`）、`wasm_modules`（hash→bytes）、`jobs`、`events`、`kv`（`namespace\0key`）
- 打开数据库时会把旧格式 `flow_versions` 键（`flow_id\0version`）迁移到十六进制前缀格式
- events 复合键 = job_id(16B) + event_id(16B)，使同一 Job 事件在 B-tree 中连续
- 错误映射统一使用 `fn s(e: impl Display) -> ShirohaError` 辅助函数
- `list_flow_versions_for` 依赖范围查询；`list_jobs` / `list_keys` 仍是全表扫描，数据量大时需关注成本
- 需导入 `redb::ReadableDatabase` 和 `redb::ReadableTable` trait

### Testing Requirements

- `cargo check -p shiroha-store-redb`
- 涉及 schema / key 编码变更时补充 reopen migration 测试
- 存储测试需使用临时文件路径，测试后清理

<!-- MANUAL: -->
