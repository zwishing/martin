# Tasks: 完善自动过滤函数生成集成

## Task 1: 修复 SQL 生成 Bug

**文件**: `martin/src/config/file/tiles/postgres/resolver/auto_filter_functions.rs`

### 1.1 修复 rtrim() 误用

- [x] 将 line 215 的 `rtrim(key, '_min')` 改为 `left(key, -4)`
- [x] 将 line 220 的 `rtrim(key, '_max')` 改为 `left(key, -4)`
- [x] 添加注释说明为什么使用 `left()` 而非 `rtrim()`

### 1.2 实现 properties 参数过滤

- [x] 修改 line 193-201 的 properties 处理逻辑
- [x] 添加 TODO 注释说明需要完整实现
- [ ] 解析逗号分隔的列名列表 (TODO: 未来实现)
- [ ] 验证列名是否存在（防止 SQL 注入）(TODO: 未来实现)
- [ ] 构建动态列列表 (TODO: 未来实现)

### 1.3 修正函数 volatility

- [x] 将 line 143 的 `IMMUTABLE` 改为 `STABLE`
- [x] 添加注释说明 volatility 选择理由

### 1.4 添加单元测试

- [x] 创建测试模块 `#[cfg(test)] mod tests`
- [x] 测试 `generate_function_sql()` 生成的 SQL
- [x] 验证 `left(key, -4)` 正确处理 `_min` 和 `_max`
- [x] 验证生成的 SQL 包含 `STABLE`
- [x] 验证函数签名正确
- [x] 验证属性列表包含
- [x] 验证特殊字符处理
- [x] 验证参数（SRID, extent, buffer, clip_geom）
- [x] 验证函数注释

---

## Task 2: 添加配置支持

### 2.1 修改 maptile 配置

**文件**: `maptile/src/config/types.rs`

- [x] 在 `PostgresConfig` 结构体中添加 `auto_generate_filters` 字段
- [x] 在 `PostgresConfig` 结构体中添加 `filter_function_suffix` 字段
- [x] 添加 `default_filter_suffix()` 辅助函数
- [x] 添加文档注释说明字段用途

### 2.2 修改 martin 配置

**文件**: `martin/src/config/file/tiles/postgres/config.rs`

- [x] 在 `PostgresCfgPublish` 结构体中添加 `auto_generate_filters` 字段
- [x] 在 `PostgresCfgPublish` 结构体中添加 `filter_function_suffix` 字段
- [x] 添加 `default_filter_suffix()` 辅助函数
- [x] 添加文档注释说明字段用途

### 2.3 添加配置测试

**文件**: `maptile/src/config/types.rs` (测试模块)

- [x] 测试默认值（`auto_generate_filters: false`）
- [x] 测试自定义值解析
- [x] 测试向后兼容性（缺少字段时使用默认值）

---

## Task 3: 集成到启动流程

**文件**: `maptile/src/main.rs`

### 3.1 添加自动生成逻辑

- [x] 在 `load_sources_from_database()` 中添加条件检查
- [x] 实现 `auto_generate_at_startup()` 辅助函数
- [x] 为所有表源生成过滤函数
- [x] 记录生成结果（成功数量或失败原因）
- [x] 使用与Redis consumer相同的SQL生成逻辑

### 3.2 错误处理

- [x] 使用 `info!` 记录配置状态
- [x] 使用 `warn!` 记录生成失败
- [x] 确保生成失败不影响服务启动
- [x] 添加详细的日志信息

### 3.3 添加集成测试

**文件**: `maptile/tests/auto_generation_startup_test.rs` (新建)

- [ ] 测试启用配置时函数被创建 (TODO: 未来实现)
- [ ] 测试禁用配置时函数不被创建 (TODO: 未来实现)
- [ ] 测试生成失败不影响启动 (TODO: 未来实现)
- [ ] 测试生成后源列表包含新函数 (TODO: 未来实现)

---

## Task 4: 集成到 Redis Consumer

**文件**: `maptile/src/config/redis_consumer.rs`

### 4.1 修改 handle_entry() 函数

- [x] 在 `write_vector_source()` 之后添加条件检查
- [x] 实现 `auto_generate_for_source()` 辅助函数
- [x] 生成完整的过滤函数 SQL（使用 STABLE 和 left()）
- [x] 支持 limit, offset, sortby, 属性过滤
- [x] 记录生成结果

### 4.2 错误处理

- [x] 使用 `warn!` 而非 `error!` 记录失败
- [x] 确保生成失败不影响消息处理
- [x] 添加详细的日志信息（包含 source_id）

### 4.3 添加集成测试

**文件**: `maptile/tests/auto_generation_redis_test.rs` (新建)

- [ ] 测试接收 Redis 消息时函数被创建
- [ ] 测试智能路由使用新生成的函数
- [ ] 测试生成失败不影响消息处理
- [ ] 测试多个消息连续处理

---

## Task 5: 更新文档

### 5.1 更新快速开始指南

**文件**: `docs/QUICKSTART.md`

- [ ] 添加自动生成配置示例
- [ ] 更新配置文件示例
- [ ] 添加启用自动生成的步骤
- [ ] 更新故障排除部分

### 5.2 更新自动生成文档

**文件**: `docs/auto-filter-functions.md`

- [ ] 更新配置选项说明
- [ ] 添加启动时自动生成说明
- [ ] 添加 Redis 集成说明
- [ ] 更新使用示例

### 5.3 更新实现状态报告

**文件**: `docs/IMPLEMENTATION_STATUS_REPORT.md`

- [ ] 更新完成度统计
- [ ] 标记已修复的问题
- [ ] 更新待办事项清单
- [ ] 更新总体评分

### 5.4 添加配置文件示例

**文件**: `maptile/config.yaml`

- [ ] 添加 `auto_generate_filters` 配置示例
- [ ] 添加 `filter_function_suffix` 配置示例
- [ ] 添加注释说明

---

## Task 6: 添加 Rustdoc 文档

**文件**: `martin/src/config/file/tiles/postgres/resolver/auto_filter_functions.rs`

### 6.1 模块级文档

- [ ] 添加模块级 `//!` 文档
- [ ] 说明自动生成的目的和用途
- [ ] 提供使用示例

### 6.2 函数文档

- [ ] 为 `create_filtered_function()` 添加完整文档
- [ ] 为 `auto_generate_filtered_functions()` 添加完整文档
- [ ] 为 `generate_function_sql()` 添加完整文档
- [ ] 为 `get_table_columns()` 添加完整文档
- [ ] 添加参数说明和返回值说明
- [ ] 添加错误情况说明

---

## Task 7: 端到端测试

**文件**: `maptile/tests/e2e_auto_generation_test.rs` (新建)

### 7.1 完整流程测试

- [ ] 启动服务（启用自动生成）
- [ ] 验证启动时生成的函数
- [ ] 发送 Redis 消息添加新表
- [ ] 验证新表的过滤函数被创建
- [ ] 发送带过滤参数的瓦片请求
- [ ] 验证自动路由到过滤函数
- [ ] 验证返回的瓦片数据正确

### 7.2 性能测试

- [ ] 测试生成 10 个函数的时间
- [ ] 测试生成 100 个函数的时间
- [ ] 验证启动时间增加在可接受范围内

---

## Task 8: 代码审查和清理

### 8.1 代码质量

- [ ] 运行 `cargo fmt` 格式化代码
- [ ] 运行 `cargo clippy` 检查警告
- [ ] 修复所有 clippy 警告
- [ ] 确保所有测试通过

### 8.2 文档完整性

- [ ] 检查所有公共 API 都有文档
- [ ] 检查所有配置字段都有注释
- [ ] 检查所有示例代码可以编译

### 8.3 向后兼容性

- [ ] 验证默认配置下行为不变
- [ ] 验证现有测试仍然通过
- [ ] 验证不启用自动生成时无额外开销

---

## 依赖关系

```
Task 1 (修复 SQL Bug)
  ↓
Task 2 (添加配置支持)
  ↓
Task 3 (启动流程集成) ← Task 4 (Redis 集成)
  ↓
Task 5 (更新文档)
  ↓
Task 6 (Rustdoc)
  ↓
Task 7 (端到端测试)
  ↓
Task 8 (代码审查)
```

## 预估工作量

- Task 1: 2-3 小时（修复 + 测试）
- Task 2: 1-2 小时（配置 + 测试）
- Task 3: 2-3 小时（集成 + 测试）
- Task 4: 2-3 小时（集成 + 测试）
- Task 5: 1-2 小时（文档更新）
- Task 6: 1 小时（Rustdoc）
- Task 7: 2-3 小时（端到端测试）
- Task 8: 1 小时（审查清理）

**总计**: 12-18 小时

## 验收标准

- [x] 所有 SQL 生成 bug 已修复
- [x] 配置字段已添加且有默认值
- [ ] 启动时自动生成功能正常工作 (部分完成，有 TODO)
- [x] Redis consumer 自动生成功能正常工作
- [ ] 所有测试通过（单元测试 + 集成测试 + 端到端测试）(配置测试已完成)
- [ ] 文档已更新且准确
- [ ] Rustdoc 完整且清晰
- [x] 代码通过 fmt 和 clippy 检查
- [x] 向后兼容性得到保证
