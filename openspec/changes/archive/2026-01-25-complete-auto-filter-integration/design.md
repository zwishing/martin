# Design: 自动过滤函数生成完整集成

## Context

智能路由功能已实现（见 `docs/IMPLEMENTATION_STATUS_REPORT.md`），测试覆盖率 100%（25/25 测试通过）。但自动生成过滤函数的代码存在多个问题，导致无法在生产环境中使用。

### 当前状态

**已完成**：
- ✅ 智能路由核心逻辑（`maptile/src/handler/smart_routing.rs`）
- ✅ 服务集成（`maptile/src/handler/tile_service.rs`）
- ✅ 完整测试覆盖（8 单元测试 + 17 集成测试）
- ✅ 详细文档（2300+ 行）
- ✅ 模块导入修复（`auto_filter_functions.rs` 已加入模块树）

**存在问题**：
- ❌ SQL 生成 bug（rtrim 误用、properties 未实现、volatility 错误）
- ❌ 缺少配置支持
- ❌ 未集成到启动流程
- ❌ 未集成到 Redis consumer

### 利益相关者

- 需要动态过滤功能的 maptile 用户
- 使用 Redis stream 实时更新数据源的系统
- 需要自动化运维的生产环境

### 约束

- 必须保持向后兼容（默认不启用）
- 必须与智能路由无缝协作
- 生成失败不应影响服务启动
- SQL 生成必须符合 PostgreSQL 标准

## Goals / Non-Goals

### Goals

1. 修复所有 SQL 生成 bug
2. 添加配置字段支持自动生成
3. 在启动时自动生成过滤函数
4. 在 Redis consumer 接收数据时触发自动生成
5. 保持向后兼容性
6. 添加完整的测试覆盖

### Non-Goals

1. 修改智能路由逻辑（已完成且稳定）
2. 支持 CQL 表达式（未来功能）
3. 自定义路由规则（未来功能）
4. 函数版本管理（未来功能）

## Decisions

### D1: SQL 生成 Bug 修复

#### Bug 1: rtrim() 误用

**问题**：
```sql
-- 当前代码
format('%I >= %L', rtrim(key, '_min'), value)

-- 问题：rtrim 移除所有指定字符，不是后缀
-- 'population_min' → rtrim('population_min', '_min') → 'populatio'
-- 因为 rtrim 移除了所有 '_', 'm', 'i', 'n' 字符
```

**修复**：
```sql
-- 使用 left() 函数正确移除后缀
format('%I >= %L', left(key, -4), value)  -- 移除最后 4 个字符 '_min'
format('%I <= %L', left(key, -4), value)  -- 移除最后 4 个字符 '_max'
```

**理由**：
- `left(str, -n)` 返回字符串去掉最后 n 个字符
- 精确移除后缀，不影响字段名中的其他字符
- PostgreSQL 标准函数，性能良好

#### Bug 2: properties 参数未实现

**问题**：
```rust
// 当前代码（auto_filter_functions.rs:194-201）
IF query_params ? 'properties' THEN
    properties_val := query_params->>'properties';
    -- Validate and build properties list
    -- For simplicity, we'll use all properties if not specified
    properties_clause := ', {properties_list}';  // 总是使用全部列
ELSE
    properties_clause := ', {properties_list}';  // 总是使用全部列
END IF;
```

**修复**：
```sql
-- 解析 properties 参数（逗号分隔的列名）
IF query_params ? 'properties' THEN
    properties_val := query_params->>'properties';
    -- 构建动态列列表
    DECLARE
        prop_array text[];
        prop text;
        selected_props text := '';
    BEGIN
        prop_array := string_to_array(properties_val, ',');
        FOREACH prop IN ARRAY prop_array
        LOOP
            -- 验证列名是否存在（防止 SQL 注入）
            IF prop = ANY(ARRAY[{properties_list_array}]) THEN
                selected_props := selected_props || ', ' || quote_ident(prop);
            END IF;
        END LOOP;
        properties_clause := selected_props;
    END;
ELSE
    properties_clause := ', {properties_list}';
END IF;
```

**理由**：
- 支持客户端选择需要的属性
- 减少数据传输量
- 防止 SQL 注入（验证列名）

#### Bug 3: 函数 volatility 错误

**问题**：
```sql
-- 当前代码
CREATE FUNCTION cities_filtered(...)
LANGUAGE plpgsql
IMMUTABLE STRICT PARALLEL SAFE  -- ❌ 错误
```

**修复**：
```sql
CREATE FUNCTION cities_filtered(...)
LANGUAGE plpgsql
STABLE STRICT PARALLEL SAFE  -- ✅ 正确
```

**理由**：
- `IMMUTABLE`：函数结果永不改变（如 `sqrt(2)`）
- `STABLE`：函数结果在事务内稳定，但可能跨事务改变（查询表数据）
- 过滤函数查询数据库表，数据可能变化，必须使用 `STABLE`
- 参考：[PostgreSQL 文档 - Function Volatility](https://www.postgresql.org/docs/current/xfunc-volatility.html)

### D2: 配置结构设计

#### maptile 配置（maptile/src/config/types.rs）

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PostgresConfig {
    pub connection_string: String,
    pub pool_size: usize,
    pub ssl_cert: Option<PathBuf>,
    pub ssl_key: Option<PathBuf>,
    pub ssl_root_cert: Option<PathBuf>,

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

#### martin 配置（martin/src/config/file/tiles/postgres/config.rs）

```rust
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PostgresCfgPublish {
    #[serde(default, skip_serializing_if = "OptOneMany::is_none")]
    pub from_schemas: OptOneMany<String>,
    #[serde(default, skip_serializing_if = "OptBoolObj::is_none")]
    pub tables: OptBoolObj<PostgresCfgPublishTables>,
    #[serde(default, skip_serializing_if = "OptBoolObj::is_none")]
    pub functions: OptBoolObj<PostgresCfgPublishFuncs>,

    /// 自动生成过滤函数（默认：false）
    #[serde(default)]
    pub auto_generate_filters: bool,

    /// 过滤函数后缀（默认："filtered"）
    #[serde(default = "default_filter_suffix")]
    pub filter_function_suffix: String,

    #[serde(flatten, skip_serializing)]
    pub unrecognized: UnrecognizedValues,
}
```

**理由**：
- 使用 `#[serde(default)]` 确保向后兼容
- 默认值为 `false`，不影响现有部署
- 后缀可配置，支持自定义命名约定

### D3: 启动时自动生成

#### 实现位置：maptile/src/main.rs

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ... 现有初始化代码 ...

    let config_pool = create_config_pool(&config.postgres).await?;

    // Initialize the service
    let service = MaptileServiceImpl::new(config.clone(), config_pool.clone()).await?;

    // Auto-generate filtered functions if enabled
    if config.postgres.auto_generate_filters {
        info!("Auto-generating filtered functions...");

        // Import the function
        use martin::config::file::tiles::postgres::resolver::{
            auto_generate_filtered_functions,
            query_available_tables,
        };

        // Get table sources from database
        match query_available_tables(&config_pool, /* schemas */ &[]).await {
            Ok(tables) => {
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
                let mut service_write = service.write().await;
                if let Err(e) = service_write.reload_sources(&config, &config_pool).await {
                    warn!("Failed to reload sources after auto-generation: {}", e);
                }
            }
            Err(e) => {
                warn!("Failed to query tables for auto-generation: {}", e);
            }
        }
    }

    let service = Arc::new(RwLock::new(service));
    // ... 继续现有代码 ...
}
```

**理由**：
- 在服务初始化后、启动前执行
- 失败不影响服务启动（使用 `warn!` 而非 `error!`）
- 生成后立即重新加载源，确保函数可用

### D4: Redis Consumer 集成

#### 实现位置：maptile/src/config/redis_consumer.rs

在 `handle_entry()` 函数中修改：

```rust
async fn handle_entry(
    pool: &Pool,
    service: &Arc<RwLock<MaptileServiceImpl>>,
    config: &MaptileConfig,
    entry: &StreamEntry,
) -> Result<(), ConsumerError> {
    let message = parse_stream_fields(&entry.fields)?;

    if message.kind != "vector" {
        info!(
            "Skipping Redis message {id} with kind '{kind}'",
            id = entry.id,
            kind = message.kind
        );
        return Ok(());
    }

    let metadata = parse_vector_metadata(&message.payload)?;
    let processed_path = resolve_processed_path(&message)?;
    let data_source = VectorDataSource::from_metadata(metadata, &processed_path)?;

    // Write vector source to database
    write_vector_source(pool, &data_source).await?;

    // Auto-generate filtered function if enabled
    if config.postgres.auto_generate_filters {
        use martin::config::file::tiles::postgres::resolver::create_filtered_function;
        use martin::config::file::postgres::TableInfo;

        let table_info = TableInfo {
            schema: data_source.schema_name.clone(),
            table: data_source.table_or_function_name.clone(),
            geometry_column: data_source.geometry_column.clone(),
            srid: data_source.srid.unwrap_or(4326),
            // 其他字段使用默认值
            extent: Some(4096),
            buffer: Some(64),
            clip_geom: Some(true),
            geometry_type: None,
            properties: data_source.properties.clone(),
        };

        let suffix = &config.postgres.filter_function_suffix;
        match create_filtered_function(pool, &table_info, suffix).await {
            Ok(function_name) => {
                info!(
                    "Generated filtered function '{}' for source '{}'",
                    function_name, data_source.source_id
                );
            }
            Err(e) => {
                warn!(
                    "Failed to generate filtered function for '{}': {}",
                    data_source.source_id, e
                );
            }
        }
    }

    // Refresh sources (includes new function if generated)
    refresh_sources(service, config, pool).await?;

    info!(
        "Applied vector source '{source_id}' from Redis message {id}",
        source_id = data_source.source_id,
        id = entry.id
    );

    Ok(())
}
```

**理由**：
- 在写入数据源后立即生成过滤函数
- 失败不影响数据源写入（使用 `warn!`）
- 刷新源时包含新生成的函数

### D5: 错误处理策略

**原则**：自动生成失败不应影响服务可用性

**实现**：
- 启动时生成失败：记录警告，继续启动
- Redis 消息处理时生成失败：记录警告，继续处理
- 智能路由回退：如果过滤函数不存在，自动使用基础表源

**理由**：
- 过滤函数是优化功能，不是核心功能
- 基础表源始终可用
- 用户可以手动创建过滤函数

## Risks

### R1: SQL 生成错误

- **风险**：修复后的 SQL 可能仍有边界情况
- **缓解**：
  - 添加完整的单元测试
  - 在测试数据库中验证生成的 SQL
  - 文档说明支持的参数类型

### R2: 性能影响

- **风险**：启动时生成大量函数可能延长启动时间
- **缓解**：
  - 默认禁用（`auto_generate_filters: false`）
  - 异步生成，不阻塞服务启动
  - 记录生成时间，便于监控

### R3: 配置迁移

- **风险**：现有配置文件需要更新
- **缓解**：
  - 所有新字段都有默认值
  - 不更新配置文件时行为不变
  - 文档提供配置示例

## Migration Plan

### Phase 1: 修复 SQL 生成（本次变更）

1. 修复 `rtrim()` bug
2. 实现 `properties` 参数
3. 修正函数 volatility
4. 添加单元测试验证修复

### Phase 2: 添加配置支持（本次变更）

1. 添加配置字段到 `maptile/src/config/types.rs`
2. 添加配置字段到 `martin/src/config/file/tiles/postgres/config.rs`
3. 添加配置解析测试

### Phase 3: 集成到启动流程（本次变更）

1. 修改 `maptile/src/main.rs`
2. 添加启动时自动生成逻辑
3. 添加集成测试

### Phase 4: 集成到 Redis Consumer（本次变更）

1. 修改 `maptile/src/config/redis_consumer.rs`
2. 添加消息处理时自动生成逻辑
3. 添加集成测试

### Phase 5: 文档更新（本次变更）

1. 更新 `docs/QUICKSTART.md`
2. 更新 `docs/auto-filter-functions.md`
3. 更新 `docs/IMPLEMENTATION_STATUS_REPORT.md`

## Open Questions

无。所有技术决策已确定。

## Testing Strategy

### 单元测试

1. **SQL 生成测试**（`auto_filter_functions.rs`）：
   - 测试 `left(key, -4)` 正确移除后缀
   - 测试 properties 参数解析
   - 测试生成的 SQL 包含 `STABLE`

2. **配置解析测试**：
   - 测试默认值
   - 测试自定义值
   - 测试向后兼容性

### 集成测试

1. **启动时自动生成测试**：
   - 启用配置，验证函数被创建
   - 禁用配置，验证函数不被创建
   - 验证生成失败不影响启动

2. **Redis consumer 测试**：
   - 接收消息，验证函数被创建
   - 验证智能路由使用新函数
   - 验证生成失败不影响消息处理

3. **端到端测试**：
   - 启动服务 → 发送 Redis 消息 → 请求瓦片（带过滤参数）
   - 验证自动路由到新生成的过滤函数
