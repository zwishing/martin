# Change: 完善自动过滤函数生成集成

## Why

当前智能路由功能已实现（`maptile/src/handler/smart_routing.rs`），可以根据查询参数自动选择过滤函数或基础表源。但自动生成过滤函数的代码（`martin/src/config/file/tiles/postgres/resolver/auto_filter_functions.rs`）存在以下问题：

1. **孤立代码**：已修复模块导入，但未集成到启动流程
2. **缺少配置支持**：无法通过配置文件启用自动生成
3. **SQL 生成错误**：
   - `rtrim()` 误用导致字段名错误（如 `population_min` → `populatio`）
   - `properties` 参数未实现
   - 函数 volatility 错误（应为 STABLE 而非 IMMUTABLE）
4. **缺少 Redis 集成**：接收 Redis stream 数据时未触发自动生成
5. **文档不一致**：文档描述的功能未完全实现

核心目标：

- 修复 SQL 生成 bug
- 添加配置字段支持自动生成
- 在启动时自动生成过滤函数
- 在 Redis consumer 接收数据时触发自动生成
- 确保智能路由与自动生成无缝协作

## What Changes

### 修复 SQL 生成 Bug

1. **修复 rtrim() 误用**：
   - 当前：`rtrim(key, '_min')` 会移除所有 `_`, `m`, `i`, `n` 字符
   - 修复：使用 `left(key, -4)` 正确移除后缀

2. **实现 properties 参数过滤**：
   - 当前：代码解析参数但始终返回所有列
   - 修复：根据 `properties` 参数动态构建列列表

3. **修正函数 volatility**：
   - 当前：`IMMUTABLE` （错误，因为查询数据库表）
   - 修复：`STABLE` （正确，结果在事务内稳定）

### 添加配置支持

#### maptile/src/config/types.rs

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PostgresConfig {
    pub connection_string: String,
    pub pool_size: usize,
    // ... 现有字段 ...

    /// 启动时自动生成过滤函数（默认：false）
    #[serde(default)]
    pub auto_generate_filters: bool,

    /// 过滤函数后缀（默认："filtered"）
    #[serde(default = "default_filter_suffix")]
    pub filter_function_suffix: String,
}

fn default_filter_suffix() -> String {
    "filtered".to_string()
}
```

#### martin/src/config/file/tiles/postgres/config.rs

```rust
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PostgresCfgPublish {
    // ... 现有字段 ...

    /// 自动生成过滤函数（默认：false）
    #[serde(default)]
    pub auto_generate_filters: bool,

    /// 过滤函数后缀（默认："filtered"）
    #[serde(default = "default_filter_suffix")]
    pub filter_function_suffix: String,
}
```

### 启动时自动生成

#### maptile/src/main.rs

在 `MaptileServiceImpl::new()` 之后添加：

```rust
// Auto-generate filtered functions if enabled
if config.postgres.auto_generate_filters {
    info!("Auto-generating filtered functions...");

    // Get table sources from database
    let tables = load_table_sources_from_database(&config_pool).await?;

    // Generate filtered functions
    let suffix = &config.postgres.filter_function_suffix;
    match auto_generate_filtered_functions(&config_pool, &tables, suffix).await {
        Ok(generated) => {
            info!("Generated {} filtered functions", generated.len());
        }
        Err(e) => {
            warn!("Failed to auto-generate filtered functions: {}", e);
        }
    }

    // Reload sources to include new functions
    service.write().await.reload_sources(&config, &config_pool).await?;
}
```

### Redis Consumer 集成

#### maptile/src/config/redis_consumer.rs

在 `handle_entry()` 函数中，`write_vector_source()` 之后添加：

```rust
// Auto-generate filtered function if enabled
if config.postgres.auto_generate_filters {
    let table_info = TableInfo {
        schema: data_source.schema_name.clone(),
        table: data_source.table_or_function_name.clone(),
        geometry_column: data_source.geometry_column.clone(),
        srid: data_source.srid.unwrap_or(4326),
        // ... 其他字段 ...
    };

    let suffix = &config.postgres.filter_function_suffix;
    match create_filtered_function(pool, &table_info, suffix).await {
        Ok(function_name) => {
            info!("Generated filtered function: {}", function_name);
        }
        Err(e) => {
            warn!("Failed to generate filtered function: {}", e);
        }
    }
}

refresh_sources(service, config, pool).await?;
```

## Impact

### 修改文件

1. **martin/src/config/file/tiles/postgres/resolver/auto_filter_functions.rs**
   - 修复 `rtrim()` → `left(key, -4)`
   - 实现 properties 参数过滤
   - 修改 `IMMUTABLE` → `STABLE`

2. **maptile/src/config/types.rs**
   - 添加 `auto_generate_filters` 字段
   - 添加 `filter_function_suffix` 字段

3. **martin/src/config/file/tiles/postgres/config.rs**
   - 添加 `auto_generate_filters` 字段到 `PostgresCfgPublish`
   - 添加 `filter_function_suffix` 字段到 `PostgresCfgPublish`

4. **maptile/src/main.rs**
   - 添加启动时自动生成逻辑

5. **maptile/src/config/redis_consumer.rs**
   - 添加 Redis 消息处理时自动生成逻辑

### 配置文件示例

```yaml
# maptile/config.yaml
postgres:
  connection_string: "postgresql://user:pass@localhost/db"
  pool_size: 10

  # 启用自动生成过滤函数
  auto_generate_filters: true

  # 自定义过滤函数后缀（可选，默认 "filtered"）
  filter_function_suffix: "filtered"
```

### 向后兼容性

- 所有新字段都有默认值（`auto_generate_filters: false`）
- 不启用时行为与当前完全一致
- 智能路由功能独立工作，不依赖自动生成

### 测试影响

需要添加测试：

1. SQL 生成测试（验证 bug 修复）
2. 配置解析测试
3. 启动时自动生成集成测试
4. Redis consumer 自动生成测试
