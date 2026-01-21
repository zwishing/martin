# Tasks: 添加 Maptile RPC 微服务

## 1. 项目结构初始化

- [ ] 1.1 创建 `maptile/` crate 目录结构
- [ ] 1.2 配置 `maptile/Cargo.toml`，添加对 `martin-core`、`martin-tile-utils` 的依赖
- [ ] 1.3 更新 workspace `Cargo.toml` 添加 maptile 成员
- [ ] 1.4 添加 Volo 相关依赖（`volo`, `volo-thrift`, `pilota`）到 workspace

## 2. Thrift IDL 定义

- [ ] 2.1 创建 `maptile/idl/maptile.thrift` IDL 文件
- [ ] 2.2 配置 `maptile/build.rs` 使用 volo-build 生成代码
- [ ] 2.3 验证 IDL 编译成功

## 3. 配置模块实现

- [ ] 3.1 从 `martin/src/config/database/` 拷贝配置类型到 `maptile/src/config/`
- [ ] 3.2 从 `martin/src/config/file/` 拷贝配置文件解析逻辑（简化版）
- [ ] 3.3 实现 `maptile/config.yaml` 配置文件结构
  - 服务监听地址（默认 0.0.0.0:8089）
  - PostgreSQL 连接字符串
  - 连接池大小
  - 热重载间隔
- [ ] 3.4 实现配置加载逻辑
- [ ] 3.5 实现配置热重载功能（定期检查数据库版本）

## 4. RPC 服务实现

- [ ] 4.1 创建 `maptile/src/server/` 模块
- [ ] 4.2 实现 `MaptileService` trait（Volo 生成的接口）
- [ ] 4.3 实现 `get_tile` 方法（调用 martin-core 的 PostgresSource）
- [ ] 4.4 实现 `list_sources` 方法
- [ ] 4.5 实现 `get_source_info` 方法
- [ ] 4.6 实现错误处理和映射到 TileError

## 5. 服务入口

- [ ] 5.1 创建 `maptile/src/main.rs` 服务入口
- [ ] 5.2 实现 CLI 参数解析（配置文件路径）
- [ ] 5.3 实现服务启动逻辑
- [ ] 5.4 实现优雅关闭
- [ ] 5.5 集成配置热重载

## 6. 验证和测试

- [ ] 6.1 验证代码编译通过 (`cargo build -p maptile`)
- [ ] 6.2 创建基础单元测试
- [ ] 6.3 创建示例配置文件
- [ ] 6.4 手动测试 RPC 服务（使用 volo 客户端）
- [ ] 6.5 验证与 PostgreSQL 数据源的连接
- [ ] 6.6 验证配置热重载功能

## 依赖关系

```
任务 1 (项目初始化)
    ↓
任务 2 (IDL 定义) ←─┬─→ 任务 3 (配置模块) [可并行]
                    ↓
              任务 4 (RPC 服务)
                    ↓
              任务 5 (服务入口)
                    ↓
              任务 6 (验证测试)
```

## 关键文件清单

| 新建文件                        | 说明                |
| ------------------------------- | ------------------- |
| `maptile/Cargo.toml`            | Crate 配置          |
| `maptile/build.rs`              | Volo 代码生成       |
| `maptile/idl/maptile.thrift`    | Thrift IDL          |
| `maptile/src/lib.rs`            | Crate 入口          |
| `maptile/src/main.rs`           | 服务入口            |
| `maptile/src/config/mod.rs`     | 配置模块            |
| `maptile/src/config/types.rs`   | 配置类型            |
| `maptile/src/config/loader.rs`  | 配置加载            |
| `maptile/src/config/reload.rs`  | 热重载逻辑          |
| `maptile/src/server/mod.rs`     | 服务模块            |
| `maptile/src/server/service.rs` | MaptileService 实现 |
| `maptile/config.yaml`           | 示例配置            |
