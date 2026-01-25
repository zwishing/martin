# maptile-rpc Specification

## Purpose
Maptile RPC is a high-performance vector tile service built on martin-core. It aligns with Martin's HTTP tile semantics, supporting composite sources, ETag-based caching, and encoding negotiation.

## Requirements

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

---

### Requirement: 条件请求支持

系统 MUST 支持基于 etag 的 conditional requests。

#### Scenario: If-None-Match 命中
- **WHEN** 客户端在请求中提供 `if_none_match`，且与当前切片 etag 匹配
- **THEN** 服务返回 `TileResponse` 且 `not_modified` 为 true
- **AND** 响应包含 etag 且 tile 数据为空

#### Scenario: If-None-Match 未命中
- **WHEN** 客户端在请求中提供 `if_none_match`，但与当前切片 etag 不匹配
- **THEN** 服务返回正常的 `TileResponse`

---

### Requirement: 复合数据源切片

系统 MUST 支持在单次请求中合并多个数据源的 MVT 切片。

#### Scenario: 合并 MVT 成功
- **WHEN** 客户端请求多个 source_id 且所有源为 MVT 且编码为 Uncompressed 或 Gzip
- **THEN** 服务返回拼接后的 MVT 数据
- **AND** 响应的 etag 为各源 etag 的拼接结果

#### Scenario: 合并不兼容格式
- **WHEN** 客户端请求多个 source_id 但存在不可合并的格式或编码
- **THEN** 服务抛出 `TileError` 异常，code 为 400

---

### Requirement: 编码协商

系统 MUST 根据 `accept_encoding` 与服务器首选编码进行编码协商。

#### Scenario: 协商成功
- **WHEN** 客户端提供 `accept_encoding` 且包含服务支持的编码
- **THEN** 服务按协商结果返回 tile 数据，并设置 content_encoding

#### Scenario: 协商失败
- **WHEN** 客户端提供 `accept_encoding` 但不包含任何服务支持的编码
- **THEN** 服务抛出 `TileError` 异常，code 为 406

---

### Requirement: 数据源列表查询

系统 MUST 提供查询所有可用数据源的 RPC 方法。

#### Scenario: 列出所有源
- **WHEN** 客户端调用 `list_sources`
- **THEN** 服务返回所有已启用数据源的 `TileInfo` 列表
- **AND** 每个 `TileInfo` 包含 source_id、name、min_zoom、max_zoom

---

### Requirement: 数据源详情查询

系统 MUST 提供查询单个数据源详情的 RPC 方法。

#### Scenario: 查询存在的源
- **WHEN** 客户端调用 `get_source_info` 并传入有效的 source_id
- **THEN** 服务返回该数据源的完整 `TileInfo`

#### Scenario: 查询不存在的源
- **WHEN** 客户端调用 `get_source_info` 并传入无效的 source_id
- **THEN** 服务抛出 `TileError` 异常，code 为 404

---

### Requirement: PostgreSQL 数据源支持

Maptile 服务 MUST 支持 PostgreSQL/PostGIS 作为切片数据源，通过复用 martin-core 的 PostgresSource。

#### Scenario: 连接 PostgreSQL
- **WHEN** 服务启动时配置了有效的 PostgreSQL 连接字符串
- **THEN** 服务成功连接数据库并初始化连接池

#### Scenario: 表源切片生成
- **WHEN** 数据源类型为 table
- **THEN** 服务通过 martin-core 的 PostgresSource 生成 MVT 数据

#### Scenario: 函数源切片生成
- **WHEN** 数据源类型为 function
- **THEN** 服务通过 martin-core 调用用户定义的 MVT 函数

---

### Requirement: 数据库配置支持

Maptile 服务 MUST 从 PostgreSQL 数据库读取数据源配置。

#### Scenario: 加载数据源配置
- **WHEN** 服务启动
- **THEN** 服务从 `martin_config.data_sources` 表读取已启用的数据源配置

#### Scenario: 配置元数据
- **WHEN** 服务启动
- **THEN** 服务从 `martin_config.metadata` 表读取配置版本信息

---

### Requirement: 配置热重载

Maptile 服务 MUST 支持配置热重载，在数据源配置变更时自动更新。

#### Scenario: 检测配置变更
- **WHEN** 服务运行中且 `martin_config.metadata` 的版本号变化
- **THEN** 服务自动重新加载数据源配置

#### Scenario: 热重载间隔可配置
- **WHEN** 配置文件设置了 `reload_interval_sec`
- **THEN** 服务按照指定间隔检查配置变更

#### Scenario: 服务关闭时停止热重载任务
- **WHEN** 服务收到关闭信号并开始退出
- **THEN** 热重载任务停止执行并释放资源

---

### Requirement: 服务配置

Maptile 服务 MUST 支持通过配置文件进行服务配置。

#### Scenario: 默认端口
- **WHEN** 未指定监听地址
- **THEN** 服务在 `0.0.0.0:8089` 启动

#### Scenario: 自定义端口
- **WHEN** 配置文件指定了 `listen_address`
- **THEN** 服务在指定地址启动

#### Scenario: 连接池大小
- **WHEN** 配置文件指定了 `pool_size`
- **THEN** PostgreSQL 连接池使用指定大小

---

### Requirement: 请求坐标校验

Maptile 服务 MUST 校验 `get_tile` 请求中的坐标是否合法。

#### Scenario: 坐标为负数或超范围
- **WHEN** 客户端发送 `get_tile` 请求，包含负数 z/x/y 或超出当前 zoom 的范围
- **THEN** 服务返回 `TileError` 异常，code 为 400

---

### Requirement: 配置标识符校验

Maptile 服务 MUST 校验数据库配置中的 schema/table/column 标识符。

#### Scenario: 标识符包含非法字符
- **WHEN** `martin_config.data_sources` 中的标识符包含非法字符
- **THEN** 服务跳过该数据源并记录 warn 日志
