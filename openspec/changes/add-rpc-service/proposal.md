# Change: 添加基于 Volo Thrift 的 RPC 微服务

## Why

将 Martin 的 HTTP 服务能力抽象为 RPC 微服务，使用字节跳动开源的 Volo 框架（CloudWeGo 生态系统）实现高性能的 Thrift RPC 服务。这将支持在微服务架构中更高效地获取矢量切片，同时保持与现有 HTTP 服务的兼容性。

核心目标：

- 提供 Thrift RPC 接口用于矢量切片获取
- 独立的 `maptile` crate 实现微服务功能（位于 Martin workspace 中）
- 仅支持 PostgreSQL 数据源配置
- 复用 `martin-core`、`martin-tile-utils`、`mbtiles` crate（直接依赖）
- 从 `martin` crate 拷贝必要的服务层代码

## What Changes

### 新增能力

- 创建 `maptile` crate 作为 Martin workspace 的新成员
- 定义 Thrift IDL 用于切片服务接口
- 实现基于 Volo 的 Thrift 服务端
- 实现配置热重载功能
- 支持配置文件方式配置服务

### 代码组织

- `maptile/` - 新的 crate 根目录（Martin workspace 成员）
  - `idl/` - Thrift IDL 定义
  - `src/` - 服务实现
    - `config/` - 配置管理（从 martin crate 拷贝并适配）
    - `server/` - Volo Thrift 服务实现

### 依赖关系

- **直接依赖**（无需拷贝）：
  - `martin-core` - 切片核心逻辑、PostgreSQL 源、Source trait
  - `martin-tile-utils` - 切片工具类型
  - `mbtiles` - MBTiles 支持（虽然本期不使用，保留依赖）
- **代码拷贝**（从 martin crate）：
  - 配置加载和热重载逻辑
  - 服务层相关代码

### 配置特性

- 使用 martin-core 的 `deadpool-postgres` 连接池
- 默认 RPC 端口：8089（可通过配置文件修改）
- PostgreSQL 连接池大小可配置
- 支持配置热重载

### 限制范围

- **仅支持**：矢量切片（MVT 格式）
- **仅支持**：PostgreSQL/PostGIS 数据源
- **仅支持**：数据库配置方式
- **不支持**：MBTiles、PMTiles、COG
- **不支持**：Sprites、Fonts、Styles
- **不支持**：文件配置方式
- **不支持**：Prometheus 监控端点

## Impact

### 新增文件

- `maptile/Cargo.toml` - Crate 配置
- `maptile/idl/maptile.thrift` - Thrift IDL 定义
- `maptile/src/lib.rs` - Crate 入口
- `maptile/src/main.rs` - 服务入口
- `maptile/src/config/` - 配置相关代码（从 martin 拷贝并适配）
- `maptile/src/server/` - RPC 服务实现
- `maptile/build.rs` - Volo 代码生成
- `maptile/config.yaml` - 示例配置文件

### Workspace 变更

- 更新 `Cargo.toml` 添加 maptile 到 workspace members
- 新增 `volo`, `volo-thrift`, `pilota` 依赖到 workspace

### 需要拷贝的代码（从 martin crate）

- `martin/src/config/database/` - 数据库配置类型和加载逻辑
- `martin/src/config/file/` - 配置文件解析（部分）
- 配置热重载相关代码
