# Tasks: 增加 Redis Stream 消费与配置刷新

## 1. 配置与依赖
- [x] 1.1 增加 Redis 连接配置结构与 YAML 解析
- [x] 1.2 增加 Redis 客户端依赖并配置 workspace

## 2. 消费与处理
- [x] 2.1 实现消费组初始化与消息消费循环
- [x] 2.2 解析消息字段（kind/payload/processed_path）
- [x] 2.3 仅在 kind=vector 时写入 data_sources 并触发内存刷新
- [x] 2.4 非法消息记录错误日志

## 3. 验证
- [x] 3.1 增加消息解析单元测试
- [x] 3.2 增加写入 data_sources 的单元测试
- [x] 3.3 运行 `cargo test -p maptile`
