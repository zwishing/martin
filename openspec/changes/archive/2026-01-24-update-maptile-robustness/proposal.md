# Change: 强化 Maptile 并发与输入安全

## Why
当前 Maptile 在高并发场景中持有锁跨 `await`，并缺少对坐标与配置标识符的严格校验，可能导致热重载延迟、无效请求行为不明确与 SQL 解析失败。

## What Changes
- **ADDED**：坐标参数合法性校验（z/x/y 范围、负值）与清晰错误码
- **ADDED**：数据库配置标识符（schema/table/column）校验，非法配置跳过并告警
- **MODIFIED**：热重载任务支持在服务关闭时可取消
- **MODIFIED**：服务并发路径避免持锁跨 `await`，减少写锁饥饿风险

## Impact
- Affected specs: `changes/add-rpc-service/specs/maptile-rpc/spec.md`
- Affected code: `maptile/src/server/service.rs`, `maptile/src/config/reload.rs`, `maptile/src/config/loader.rs`
