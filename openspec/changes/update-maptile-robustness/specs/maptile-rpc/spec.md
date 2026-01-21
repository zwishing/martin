## ADDED Requirements

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

---

## MODIFIED Requirements

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
