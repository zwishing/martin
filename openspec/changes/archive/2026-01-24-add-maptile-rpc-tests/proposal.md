# Change: 增加 Maptile RPC 单元测试覆盖

## Why
当前缺少对 `get_source_info` 与 `list_sources` 行为的单元测试覆盖，影响回归保障与变更信心。

## What Changes
- **ADDED**：为 `list_sources` 与 `get_source_info` 添加单元测试，验证 TileInfo 字段与错误路径

## Impact
- Affected specs: `changes/add-rpc-service/specs/maptile-rpc/spec.md`
- Affected code: `maptile/src/server/service.rs`
