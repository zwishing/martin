# Change: 增加 Redis Stream 消费与配置刷新

## Why
当前 Maptile 仅依赖数据库轮询刷新数据源配置，无法及时响应外部系统推送的 dataset 结果事件。

## What Changes
- **ADDED**：Redis Stream 消费组 `dataset-result-maptile-consumers`，监听 `dataset:result`
- **ADDED**：解析消息并在 `kind=vector` 时写入 `martin_config.data_sources`
- **ADDED**：消费到有效消息后刷新内存配置（与数据库变更一致）
- **ADDED**：错误消息仅记录日志，不影响服务运行

## Impact
- Affected specs: `changes/add-rpc-service/specs/maptile-rpc/spec.md`
- Affected code: `maptile/src/main.rs`, `maptile/src/config/reload.rs`, `maptile/src/config/loader.rs`, `maptile/src/config/types.rs`
