# 快速开始：5 分钟启用智能路由

## 🚀 目标

在 5 分钟内为你的 maptile 服务启用智能过滤路由功能。

---

## 📋 前提条件

- ✅ 已安装 PostgreSQL + PostGIS
- ✅ 已部署 maptile RPC 服务
- ✅ 有至少一个表源（如 `cities`）

---

## 🎯 Step 1: 创建过滤函数 (2 分钟)

### 连接数据库

```bash
psql -d your_database
```

### 创建过滤函数

```sql
-- 为 cities 表创建过滤函数
CREATE OR REPLACE FUNCTION cities_filtered(
    z integer,
    x integer,
    y integer,
    query_params json DEFAULT '{}'::json
)
RETURNS bytea
LANGUAGE plpgsql
IMMUTABLE STRICT PARALLEL SAFE
AS $$
DECLARE
    mvt bytea;
    limit_val integer;
    offset_val integer;
    where_clauses text[] := ARRAY['geom && ST_Transform(ST_TileEnvelope(z, x, y), 4326)'];
    order_clause text := '';
BEGIN
    -- 解析 limit
    limit_val := COALESCE((query_params->>'limit')::integer, 10000);
    IF limit_val > 100000 THEN limit_val := 100000; END IF;

    -- 解析 offset
    offset_val := COALESCE((query_params->>'offset')::integer, 0);

    -- 解析 sortby
    IF query_params ? 'sortby' THEN
        DECLARE
            sortby_val text := query_params->>'sortby';
        BEGIN
            IF sortby_val LIKE '-%' THEN
                order_clause := format(' ORDER BY %I DESC', ltrim(sortby_val, '-'));
            ELSE
                order_clause := format(' ORDER BY %I ASC', ltrim(sortby_val, '+'));
            END IF;
        END;
    END IF;

    -- 属性过滤: population_min
    IF query_params ? 'population_min' THEN
        where_clauses := array_append(
            where_clauses,
            format('population >= %L', (query_params->>'population_min')::integer)
        );
    END IF;

    -- 属性过滤: population_max
    IF query_params ? 'population_max' THEN
        where_clauses := array_append(
            where_clauses,
            format('population <= %L', (query_params->>'population_max')::integer)
        );
    END IF;

    -- 执行查询
    EXECUTE format($sql$
        SELECT ST_AsMVT(tile, 'cities', 4096, 'geom', 'id')
        FROM (
            SELECT
                ST_AsMVTGeom(
                    ST_Transform(geom, 3857),
                    ST_TileEnvelope(%s, %s, %s),
                    4096, 64, true
                ) AS geom,
                id, name, population
            FROM cities
            WHERE %s
            %s
            LIMIT %s OFFSET %s
        ) AS tile
        WHERE geom IS NOT NULL
    $sql$,
        z, x, y,
        array_to_string(where_clauses, ' AND '),
        order_clause,
        limit_val,
        offset_val
    ) INTO mvt;

    RETURN COALESCE(mvt, ''::bytea);
END;
$$;

-- 添加注释
COMMENT ON FUNCTION cities_filtered IS
'Smart routing enabled filtered tile function.
Supports: limit, offset, sortby, population_min, population_max';
```

### 验证函数

```sql
-- 测试函数是否正常工作
SELECT length(cities_filtered(0, 0, 0, '{"limit": 10}'::json)) as tile_size;

-- 应该返回一个正数（瓦片大小）
```

---

## 🔧 Step 2: 注册函数源 (1 分钟)

### 编辑配置文件

```bash
vim /path/to/maptile/config.yaml
```

### 添加函数源

```yaml
postgres:
  connection_string: "postgresql://user:pass@localhost/db"

  # 现有的表源配置
  tables:
    cities:
      schema: public
      table: cities
      geometry_column: geom
      srid: 4326

  # 新增：注册过滤函数
  functions:
    cities_filtered:
      schema: public
      function: cities_filtered
```

---

## 🔄 Step 3: 重启服务 (1 分钟)

```bash
# 重启 maptile 服务
systemctl restart maptile

# 或使用 Docker
docker-compose restart maptile

# 或手动启动
maptile --config config.yaml
```

---

## ✅ Step 4: 测试 (1 分钟)

### 测试无过滤参数（应使用静态表源）

```bash
curl "http://localhost:8089/cities/0/0/0.mvt" -o /tmp/test1.mvt
ls -lh /tmp/test1.mvt
# 应该返回瓦片数据
```

### 测试有过滤参数（应自动路由到过滤函数）

```bash
curl "http://localhost:8089/cities/0/0/0.mvt?limit=10" -o /tmp/test2.mvt
ls -lh /tmp/test2.mvt
# 应该返回更小的瓦片（只有 10 个特征）
```

### 测试属性过滤

```bash
curl "http://localhost:8089/cities/0/0/0.mvt?population_min=1000000&limit=50" -o /tmp/test3.mvt
ls -lh /tmp/test3.mvt
# 应该只返回人口 >= 100万的城市
```

### 检查日志

```bash
# 查看路由决策日志
tail -f /var/log/maptile/maptile.log | grep "Resolved source"

# 应该看到类似输出：
# [INFO] Resolved source 'cities' → 'cities' (has_filters: false)
# [INFO] Resolved source 'cities' → 'cities_filtered' (has_filters: true)
```

---

## 🎉 完成！

现在你的 maptile 服务已经支持智能路由：

- ✅ 无过滤参数 → 自动使用静态表源（最快）
- ✅ 有过滤参数 → 自动使用过滤函数（优化后）
- ✅ 客户端无需修改代码

---

## 📊 性能对比

### 测试性能提升

```bash
# 安装 Apache Bench
sudo apt-get install apache2-utils

# 测试无过滤（基准）
ab -n 1000 -c 10 "http://localhost:8089/cities/14/8192/5461.mvt"

# 测试有过滤（应该更快）
ab -n 1000 -c 10 "http://localhost:8089/cities/14/8192/5461.mvt?limit=10"
```

### 预期结果

| 场景 | 平均响应时间 | 提升 |
|------|-------------|------|
| 无过滤 | ~5ms | 基准 |
| limit=10 | ~0.8ms | **+525%** |
| limit=100 | ~2ms | **+160%** |
| 属性过滤 | ~1.2ms | **+317%** |

---

## 🔍 故障排除

### 问题 1: 函数创建失败

**错误**: `ERROR: function cities_filtered already exists`

**解决**:
```sql
-- 删除旧函数
DROP FUNCTION IF EXISTS cities_filtered(integer, integer, integer, json);

-- 重新创建
-- (粘贴上面的 CREATE FUNCTION 语句)
```

### 问题 2: 路由不生效

**症状**: 有过滤参数但仍返回所有数据

**检查**:
```bash
# 1. 确认函数已注册
curl "http://localhost:8089/catalog.json" | jq '.functions[] | select(.id == "cities_filtered")'

# 2. 检查日志
tail -f /var/log/maptile/maptile.log | grep "cities"

# 3. 手动测试函数
psql -d mydb -c "SELECT length(cities_filtered(0, 0, 0, '{\"limit\": 10}'::json))"
```

### 问题 3: 性能没有提升

**原因**: 缺少索引

**解决**:
```sql
-- 添加索引
CREATE INDEX CONCURRENTLY idx_cities_population ON cities(population);
CREATE INDEX CONCURRENTLY idx_cities_geom ON cities USING GIST(geom);

-- 更新统计信息
ANALYZE cities;

-- 重新测试
```

---

## 📚 下一步

### 添加更多过滤参数

```sql
-- 修改函数，添加时间过滤
IF query_params ? 'datetime_from' THEN
    where_clauses := array_append(
        where_clauses,
        format('updated_at >= %L', (query_params->>'datetime_from')::timestamptz)
    );
END IF;
```

### 为其他表创建过滤函数

```sql
-- 复制 cities_filtered 函数
-- 修改表名和字段名
CREATE FUNCTION roads_filtered(...) RETURNS bytea AS $$
    -- 修改 FROM cities 为 FROM roads
    -- 修改字段名
$$;
```

### 启用自动生成 (可选)

如果你不想手动创建每个过滤函数，可以在配置文件中启用自动生成：

```yaml
postgres:
  auto_generate_filters: true  # 自动为所有表生成过滤函数
  filter_function_suffix: "filtered" # 生成函数的后缀 (可选)
```

---

## 🔍 故障排除

### 性能优化

```sql
-- 1. 为常用过滤字段创建索引
CREATE INDEX idx_cities_population ON cities(population);

-- 2. 使用部分索引（如果只过滤特定范围）
CREATE INDEX idx_cities_large_pop ON cities(population)
WHERE population >= 1000000;

-- 3. 定期更新统计信息
ANALYZE cities;
```

### 监控

```bash
# 监控路由决策
tail -f /var/log/maptile/maptile.log | grep "Resolved source"

# 监控查询性能
psql -d mydb -c "SELECT * FROM pg_stat_statements WHERE query LIKE '%cities_filtered%' ORDER BY mean_exec_time DESC LIMIT 10"
```

### 调试

```bash
# 启用详细日志
RUST_LOG=debug maptile --config config.yaml

# 测试特定瓦片
curl -v "http://localhost:8089/cities/14/8192/5461.mvt?limit=10"
```

---

## 🎓 学习资源

- [完整文档](./smart-routing.md) - 详细的使用指南
- [实现总结](./IMPLEMENTATION_SUMMARY.md) - 技术实现细节
- [自动生成函数](./auto-filter-functions.md) - 高级功能

---

## ✅ 检查清单

- [ ] 创建过滤函数
- [ ] 注册函数源
- [ ] 重启服务
- [ ] 测试无过滤参数
- [ ] 测试有过滤参数
- [ ] 检查日志
- [ ] 性能测试
- [ ] 添加索引
- [ ] 更新客户端代码（可选）

---

**恭喜！你已经成功启用智能路由功能！** 🎉

现在你的客户端可以透明地使用过滤功能，无需关心底层实现细节。
