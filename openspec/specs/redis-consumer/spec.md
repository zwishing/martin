# redis-consumer Specification

## Purpose
TBD - created by archiving change add-redis-consumer. Update Purpose after archive.
## Requirements
### Requirement: Redis Stream 消费

Maptile 服务 MUST 通过 Redis Stream 消费组 `dataset-result-maptile-consumers` 监听 `dataset:result`。

#### Scenario: 启动后监听消息
- **WHEN** 服务启动并配置了 Redis 连接
- **THEN** 服务创建/加入消费组并开始监听新消息

---

### Requirement: Vector Metadata 写入

Maptile 服务 MUST 在收到 `kind=vector` 的消息时写入 `martin_config.data_sources`。

#### Scenario: 有效 vector 消息
- **WHEN** 消息包含 `kind=vector` 且 `payload` 为有效 `VectorMetadata`
- **THEN** 服务写入 `schema_name=vector`
- **AND** `table_or_function_name` 使用 `processed_path`
- **AND** `geometry_column=geom`，`id_column=gid`

---

### Requirement: 配置刷新

Maptile 服务 MUST 在成功写入后刷新内存配置。

#### Scenario: 写入后刷新
- **WHEN** 写入 `data_sources` 成功
- **THEN** 服务刷新内存配置以立即生效

---

### Requirement: 错误处理

Maptile 服务 MUST 在消息格式错误时仅记录错误日志。

#### Scenario: 无效消息
- **WHEN** 消息字段缺失或 `payload` 解析失败
- **THEN** 服务记录错误日志并继续消费

