# Design: Maptile RPC 微服务架构

## Context

Martin 目前通过 HTTP (Actix-Web) 提供切片服务。在微服务架构中，RPC 通常比 HTTP 有更低的延迟和更高的吞吐量。Volo 是字节跳动开发的高性能 Rust RPC 框架，支持 Thrift 和 gRPC 协议，在 CloudWeGo 生态系统中广泛使用。

### 利益相关者

- 需要集成切片服务到微服务架构的用户
- 需要高性能 RPC 接口的后端服务
- 使用字节跳动技术栈的团队

### 约束

- 必须使用 Volo 框架和 Thrift 协议
- 必须作为 Martin workspace 的成员
- 直接依赖 `martin-core`、`martin-tile-utils`、`mbtiles`
- 仅从 `martin` crate 拷贝必要代码
- 仅支持 PostgreSQL 数据源和矢量切片
- 使用 martin-core 的 `deadpool-postgres` 连接池

## Goals / Non-Goals

### Goals

1. 提供高性能的 Thrift RPC 切片服务
2. 作为 Martin workspace 成员，复用现有 crate
3. 支持 PostgreSQL/PostGIS 数据源
4. 支持数据库驱动的配置方式
5. 实现配置热重载功能
6. 支持配置文件配置（端口、连接池等）
7. 默认端口 8089，可配置

### Non-Goals

1. 支持 MBTiles/PMTiles/COG 文件源
2. 支持 Sprites/Fonts/Styles 资源
3. HTTP API 兼容层
4. Prometheus 监控端点
5. 文件配置方式加载数据源

## Decisions

### D1: Thrift IDL 设计

使用简洁的 Thrift 接口定义切片服务：

```thrift
namespace rs maptile

struct TileCoord {
    1: required i16 z,
    2: required i64 x,
    3: required i64 y,
}

struct TileRequest {
    1: required string source_id,
    2: required TileCoord coord,
    3: optional map<string, string> query_params,
}

struct TileResponse {
    1: required binary data,
    2: required string content_type,
    3: optional string content_encoding,
    4: optional string etag,
}

struct TileInfo {
    1: required string source_id,
    2: required string name,
    3: optional i32 min_zoom,
    4: optional i32 max_zoom,
    5: optional string bounds,
}

exception TileError {
    1: required i32 code,
    2: required string message,
}

service MaptileService {
    TileResponse get_tile(1: TileRequest request) throws (1: TileError error),
    list<TileInfo> list_sources() throws (1: TileError error),
    TileInfo get_source_info(1: string source_id) throws (1: TileError error),
}
```

**理由**：

- 接口简洁，覆盖核心切片获取功能
- 返回二进制数据和元信息
- 支持查询参数传递

### D2: 依赖策略

```
maptile (新 crate)
  ├── martin-core (直接依赖)
  │   └── PostgresSource, Source trait, TileInfo 等
  ├── martin-tile-utils (直接依赖)
  │   └── TileCoord, TileData, Encoding 等
  └── [代码拷贝自 martin crate]
      └── 配置加载、热重载逻辑
```

**理由**：

- 最大化代码复用
- 减少重复维护成本
- 仅拷贝服务层相关代码

### D3: 配置文件格式

```yaml
# maptile/config.yaml
server:
  listen_address: "0.0.0.0:8089"

postgres:
  connection_string: "postgresql://user:pass@localhost:5432/db"
  pool_size: 10

config:
  source: database # 仅支持 database
  reload_interval_sec: 60 # 热重载间隔
```

**理由**：

- 与 Martin 配置风格一致
- 简化的配置项（仅必需项）
- 支持热重载配置

### D4: 数据库连接

直接使用 martin-core 的 `deadpool-postgres` 连接池：

**理由**：

- 与 martin-core 的 PostgresPool 完全兼容
- 无需适配层
- 复用已有的连接管理逻辑

### D5: 配置热重载

复用 Martin 的热重载机制：

1. 定期从 `martin_config.metadata` 检查版本
2. 版本变化时重新加载数据源配置
3. 使用 `Arc<RwLock<>>` 共享状态

### R1: 代码拷贝维护

- **风险**：配置代码拷贝后需要手动同步
- **缓解**：文档记录拷贝来源，定期检查

### R2: 功能子集

- **风险**：仅支持 PostgreSQL 可能限制使用场景
- **缓解**：按需扩展，保持接口可扩展性

## Migration Plan

### Phase 1: 基础实现（本次变更）

1. 创建 maptile crate 结构
2. 定义 Thrift IDL
3. 实现 RPC 服务端
4. 实现配置热重载
5. 验证与 PostgreSQL 数据源的连接

### Phase 2: 未来扩展（不在本次范围）

- 支持更多数据源类型
- 添加缓存层
- 客户端 SDK

## Open Questions

无。所有关键技术决策已确定。
