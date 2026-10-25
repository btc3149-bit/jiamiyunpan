# 备份与恢复指南

甲米云盘的所有持久状态都集中在 `DATA_DIR`（容器内默认 `/data`）。端到端加密意味着
**服务端没有主密钥**——备份 `DATA_DIR` 就备份了一切；但反过来，客户端的私钥
（账户密钥）只存在于用户浏览器/设备里，服务端备份救不回忘密码的用户。

## DATA_DIR 目录结构

```text
DATA_DIR/
├── sqlite.db              # SQLite 数据库（默认后端；元数据、用户、分享、笔记等）
├── sqlite.db-shm          # SQLite WAL 伴生文件（存在时）
├── sqlite.db-wal          # 同上
├── jiamiyunpan.crt.pem    # 自签证书（未挂载 SSL_CERT_FILE 时自动生成）
├── jiamiyunpan.key.pem    # 自签私钥（同上）
└── {file_id}/             # 每个文件一个目录
    ├── 000001.chunk       # 旧版扁平布局的密文块（历史数据仍可读）
    └── v1/                # 版本化布局：每个已提交版本一个 v{N} 子目录
    │   ├── 000001.chunk   # 密文块，文件名是块序号（零填充 6 位）
    │   └── 000002.chunk
    └── v2/                # 编辑过一次后出现；读哪个版本由数据库行决定
```

要点：

- **数据库与块文件必须一起备份**。数据库记录块的版本与序号，块文件是密文本体，
  只备一半等于没备。
- 切到 S3 后端（`STORAGE_PROVIDER=s3`）时块文件进桶，`DATA_DIR` 里只剩数据库、
  证书和本地状态——此时数据库备份更加关键。
- SQLite 与 PostgreSQL 不通用。若用了 PostgreSQL（`DATABASE_URL`），备份对象换成
  `pg_dump`，本文件的 SQLite 部分不适用。

## SQLite 备份方法

### 推荐：sqlite3 .backup（在线一致备份）

```shell
# 容器内没有 sqlite3，在宿主机上对挂载目录操作（先安装 sqlite3）
sqlite3 /path/to/data/sqlite.db ".backup '/path/to/backups/sqlite-$(date +%F).db'"
```

`.backup` 命令通过 SQLite 的备份 API 拿到一致快照，服务运行中执行也安全。
WAL 模式下不要用 `cp` 热拷贝 `sqlite.db`——很容易拷到半写状态。

### 次选：停机冷备（cp）

```shell
docker compose -f docker-compose.prod.yml stop app
cp -a /path/to/data /path/to/backups/data-$(date +%F)
docker compose -f docker-compose.prod.yml start app
```

停机后整目录 `cp`（含 WAL/SHM 伴生文件）最简单也最可靠，适合脚本化的定时备份窗口。

## S3 用户注意事项

- **块数据不在 `DATA_DIR`**，在桶里。数据库备份只覆盖元数据。
- S3 侧的容灾取决于提供商（版本控制、跨区域复制），按提供商文档配置；至少确认
  桶没有误开公开访问。
- 桶里的对象键同样是 `{file_id}/v{N}/{chunk:06}.chunk` 布局，**不要**手工整理或
  重命名对象——数据库里的引用会全部失效。
- 误删了桶内容而数据库还在时，受影响文件会下载失败；先停写、恢复对象，再放行。
- 从本地迁移到 S3 用的是 `jiamiyunpan migrate-storage`（幂等可重跑），迁移前后
  各做一次完整备份。

## 恢复步骤

```shell
# 1. 停服务
docker compose -f docker-compose.prod.yml stop app

# 2. 恢复 DATA_DIR（整目录或至少 sqlite.db* + 全部 {file_id}/）
rm -rf /path/to/data && cp -a /path/to/backups/data-YYYY-MM-DD /path/to/data

# 若只恢复数据库：把备份的 sqlite.db（连同 -wal/-shm）放回 DATA_DIR，
# 并确认 {file_id}/ 块目录没有被更新版本的备份覆盖错位。

# 3. 修属主（非 root 容器，UID/GID 1000）
chown -R 1000:1000 /path/to/data

# 4. 起服务并确认健康
docker compose -f docker-compose.prod.yml up -d
docker inspect --format '{{.State.Health.Status}}' jiamiyunpan
curl -sk https://localhost:5443/api/readiness
```

恢复到另一台机器：把 `DATA_DIR` + compose 文件 + `.env`（`APP_URL`/`JWT_SECRET`/
`SSL_CERT_FILE` 等）搬过去即可。**`JWT_SECRET` 与证书私钥必须随迁**——换了
`JWT_SECRET` 所有用户会话失效，换了 TLS 私钥浏览器可能报证书不匹配。

## 升级失败回滚

⚠️ **回滚前必须先备份 DB。** 数据库 migration 用 SeaORM，只有 `up` 没有
`down`——新版本启动时把 schema 向前迁移后，**直接换回旧镜像启动，旧代码面对
新 schema 可能报错甚至写坏数据**。回滚流程：

```shell
# 1. 停服务
docker compose -f docker-compose.prod.yml stop app

# 2. 先备份当前（已迁移 schema 的）数据库
sqlite3 /path/to/data/sqlite.db ".backup '/path/to/backups/sqlite-before-rollback.db'"

# 3. 换回旧版本镜像启动
docker compose -f docker-compose.prod.yml up -d \
  # （先把 compose 里的 image tag 改回上一个已验证版本）

# 4. 观察启动日志
docker compose -f docker-compose.prod.yml logs -f app
```

- 如果旧版本能正常跑：数据可用，但 schema 停留在新版本状态，之后升级无碍。
- 如果旧版本起不来（schema 不兼容）：停服，恢复第 2 步之前的数据库备份
  （即旧 schema 时代的备份），再起旧版本。这意味着回滚会丢失新版本运行期间
  产生的数据——这也是**升级前先备份**的原因。
- 稳妥的做法：每次升级前整目录备份 `DATA_DIR`，镜像 tag 固定为具体版本而非
  浮动标签，回滚时才有确定落点。
