## MODIFIED Requirements

### Requirement: Maptile RPC 服务

系统 MUST 提供基于 Volo Thrift 的 RPC 微服务，用于获取矢量切片。

#### Scenario: 获取切片成功
- **WHEN** 客户端发送 `get_tile` 请求，包含有效的 source_id 和坐标
- **THEN** 服务返回包含 MVT 数据的 `TileResponse`
- **AND** 响应包含正确的 content_type ("application/vnd.mapbox-vector-tile")
- **AND** 响应包含 etag，使用与 martin-core 一致的哈希算法
- **AND** 当请求包含 `accept_encoding` 时，响应的 content_encoding 与协商结果一致

#### Scenario: 源不存在
- **WHEN** 客户端请求不存在的 source_id
- **THEN** 服务抛出 `TileError` 异常，code 为 404

#### Scenario: 坐标超出范围
- **WHEN** 客户端请求的 zoom 级别超出数据源配置范围
- **THEN** 服务返回空的 tile 数据
- **AND** 响应包含 etag

## ADDED Requirements

### Requirement: 条件请求支持

系统 MUST 支持基于 etag 的条件请求。

#### Scenario: If-None-Match 命中
- **WHEN** 客户端在请求中提供 `if_none_match`，且与当前切片 etag 匹配
- **THEN** 服务返回 `TileResponse` 且 `not_modified` 为 true
- **AND** 响应包含 etag 且 tile 数据为空

#### Scenario: If-None-Match 未命中
- **WHEN** 客户端在请求中提供 `if_none_match`，但与当前切片 etag 不匹配
- **THEN** 服务返回正常的 `TileResponse`

### Requirement: 复合数据源切片

系统 MUST 支持在单次请求中合并多个数据源的 MVT 切片。

#### Scenario: 合并 MVT 成功
- **WHEN** 客户端请求多个 source_id 且所有源为 MVT 且编码为 Uncompressed 或 Gzip
- **THEN** 服务返回拼接后的 MVT 数据
- **AND** 响应的 etag 为各源 etag 的拼接结果

#### Scenario: 合并不兼容格式
- **WHEN** 客户端请求多个 source_id 但存在不可合并的格式或编码
- **THEN** 服务抛出 `TileError` 异常，code 为 400

### Requirement: 编码协商

系统 MUST 根据 `accept_encoding` 与服务器首选编码进行编码协商。

#### Scenario: 协商成功
- **WHEN** 客户端提供 `accept_encoding` 且包含服务支持的编码
- **THEN** 服务按协商结果返回 tile 数据，并设置 content_encoding

#### Scenario: 协商失败
- **WHEN** 客户端提供 `accept_encoding` 但不包含任何服务支持的编码
- **THEN** 服务抛出 `TileError` 异常，code 为 406
