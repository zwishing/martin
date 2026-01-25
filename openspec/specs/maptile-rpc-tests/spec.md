# maptile-rpc-tests Specification

## Purpose
TBD - created by archiving change add-maptile-rpc-tests. Update Purpose after archive.
## Requirements
### Requirement: RPC 行为测试覆盖

Maptile 服务 MUST 提供覆盖 `list_sources` 与 `get_source_info` 的单元测试。

#### Scenario: list_sources 返回 TileInfo

- **WHEN** 以已注册的数据源调用 `list_sources`
- **THEN** 单元测试验证返回的 `TileInfo` 字段与 TileJSON 一致

#### Scenario: get_source_info 查询存在的源

- **WHEN** 以已注册的数据源调用 `get_source_info`
- **THEN** 单元测试验证返回的 `TileInfo` 字段与 TileJSON 一致

#### Scenario: get_source_info 查询不存在的源

- **WHEN** 以不存在的 source_id 调用 `get_source_info`
- **THEN** 单元测试验证返回 404 错误

