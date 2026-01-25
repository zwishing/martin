## Context
新增 Redis Stream 消费能力，用于接收外部系统推送的 dataset 处理结果并更新 Maptile 数据源配置。

## Goals / Non-Goals

### Goals
- 在 Maptile 中增加 Redis Stream 消费组
- 解析消息并写入 `martin_config.data_sources`
- 触发内存配置刷新
- 无效消息仅记录错误日志

### Non-Goals
- 修改 Thrift IDL
- 修改 tile 生成逻辑
- 引入复杂的消息重试/死信队列

## Decisions
- **D1: 配置位置**：在 `maptile/config.yaml` 增加 Redis 连接配置
- **D2: 消费组**：固定为 `dataset-result-maptile-consumers`，stream 为 `dataset:result`
- **D3: 消息格式**：读取字段 `payload` 作为 `VectorMetadata` JSON；`kind` 仅处理 `vector`
- **D4: 表名规则**：`schema_name` 固定为 `vector`，`table_or_function_name` 来自 `processed_path`
- **D5: 列规则**：`geometry_column=geom`，`id_column=gid`
- **D6: 刷新策略**：写入数据库后更新版本号并触发内存刷新

## Risks / Trade-offs
- 新增 Redis 依赖与配置项
- 不处理非 vector 的消息，避免误写入

## Migration Plan
1. 更新配置文件，填写 Redis 连接信息
2. 启动服务，确认消费组创建并可处理消息
3. 观察数据库更新与内存刷新日志

## Open Questions
- 无。
