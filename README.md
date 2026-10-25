# 甲米云盘

<p align="center">
  <img src="./web/public/android-icon-192x192.png" alt="甲米云盘" width="128" />
</p>

<p align="center">
  自托管的端到端加密云盘 · 浏览器内加解密 · 服务端永远拿不到明文
</p>

<p align="center">
  <a href="./LICENSE.md"><img src="https://img.shields.io/badge/license-CC%20BY--NC%204.0-lightgrey.svg" alt="CC BY-NC 4.0" /></a>
</p>

---

## 这是什么

甲米云盘是一个轻量、可自托管的端到端加密文件存储服务。后端 Rust（Actix-web）+ SeaORM，
前端 Vue 3 + TypeScript，加密全部在浏览器里完成，服务端只负责存密文。

- 浏览器端加密：AEGIS-128L / AEGIS-256 / Ascon-128a / ChaCha20-Poly1305
- 文件密钥封装：X25519 + ML-KEM-768 混合（抗量子），老账户兼容 RSA-2048
- 登录：OPAQUE（RFC 9807），密码永不出设备
- 存储：本地磁盘或任意 S3 兼容对象存储
- 数据库：SQLite 开箱即用，可切 PostgreSQL

## 功能

| 能力 | 说明 |
|------|------|
| 端到端加密 | 分块 AES 系/AEGIS AEAD 加密，密钥只存在于客户端 |
| 加密搜索 | 元数据分词后 HMAC 打标，服务端只能匹配标签，读不出关键词 |
| 加密笔记 | Markdown 所见即所得编辑器，内容加密、自动保存、可搜索 |
| 公开分享链接 | 密钥放在 URL fragment，服务端全程不解密 |
| 双因素认证 | 每用户可选 TOTP |
| 管理后台 | 用户、会话、邀请、全局设置 |
| 分块传输 | 并发上传下载，支持 `?format=tar` 批量搬运 |
| 直连对象存储 | `S3_DIRECT_TRANSFER` 让浏览器直连桶，服务端不中转流量 |

## 加密设计

### 账户密钥

注册时客户端生成两套密钥：

- **Ed25519** 身份密钥 —— 用于签名（分享请求、审计链、目录成员列表）
- **X25519 + ML-KEM-768** 封装密钥 —— 用于封装文件密钥，混合构造保证即使量子计算机
  攻破椭圆曲线那一半，ML-KEM 那一半仍然成立

私钥用 OPAQUE `export_key` 派生出的 KEK（HKDF-SHA512）做信封加密后存服务端；服务端没有密码，
也就永远解不开。老账户的 RSA-2048 密钥对在下次登录时自动迁移。

> ⚠️ 私钥请自行备份到密码管理器。忘记密码 = 丢私钥 = 文件不可恢复。

### 上传流程

1. 客户端为文件生成随机对称密钥
2. 按块加密，每块带 CRC16 校验后上传
3. 密文块落盘或进 S3，密钥用接收方公钥封装后入库

上传端点：

- `POST /api/storage/{file_id}?chunk=N&checksum=...` —— 单块上传
- `POST /api/storage/{file_id}?format=tar` —— 多块打包成未压缩 tar，减少往返

### 搜索

文件名与笔记内容在浏览器里分词，每个 token 用私钥派生的 HMAC 密钥打标，只上传标签。
查询时对 query 做同样处理，服务端匹配标签。数据库被拖走也无法还原成词。

### 公开链接

1. 生成随机 link key
2. 用 link key 加密文件元数据和文件密钥
3. link key 再用分享者自己的公钥封装
4. link key 作为 fragment 拼在 URL 后面：`https://…/links/{id}#link-key`

fragment 不会进 HTTP 请求，服务端只吐密文。

## 快速开始

### Docker

```shell
docker run --name jiamiyunpan -d \
  -e DATA_DIR='/data' \
  -e APP_URL='https://drive.example.com' \
  -v "$(pwd)/data:/data" \
  -p 5443:5443 \
  ghcr.io/btc3149-bit/jiamiyunpan:unstable-amd64
```

镜像发布在 GHCR：`ghcr.io/btc3149-bit/jiamiyunpan`。`unstable-amd64` 标签是
多架构 manifest，覆盖 `amd64` / `armv6` / `armv7` / `arm64` 四种架构，
Docker 会按宿主机架构自动拉取对应镜像。

首次启动会在 `DATA_DIR` 自动生成自签证书。生产环境请挂载自己的证书，或放在 Nginx / Caddy
反代后面。

### 带邮件与自有证书

```shell
docker run --name jiamiyunpan -d \
  -e DATA_DIR='/data' \
  -e APP_URL='https://drive.example.com' \
  -e SSL_CERT_FILE='/data/fullchain.pem' \
  -e SSL_KEY_FILE='/data/privkey.pem' \
  -e MAILER_TYPE='smtp' \
  -e SMTP_ADDRESS='smtp.example.com' \
  -e SMTP_USERNAME='noreply@example.com' \
  -e SMTP_PASSWORD='***' \
  -e SMTP_PORT='465' \
  -e SMTP_DEFAULT_FROM_EMAIL='noreply@example.com' \
  -e SMTP_DEFAULT_FROM_NAME='甲米云盘' \
  -v "$(pwd)/data:/data" \
  -p 5443:5443 \
  ghcr.io/btc3149-bit/jiamiyunpan:unstable-amd64
```

`JWT_SECRET` 一定要设成固定随机串，否则容器重启后所有会话失效。

### Docker Compose

仓库提供 [docker-compose.prod.yml](./docker-compose.prod.yml) 用于生产部署
（默认 SQLite，可选 Postgres 服务以注释形式给出）：

```shell
# 先编辑环境变量（APP_URL / JWT_SECRET 必填，建议用 .env 文件）
curl -O https://raw.githubusercontent.com/btc3149-bit/jiamiyunpan/main/docker-compose.prod.yml

docker compose -f docker-compose.prod.yml up -d
docker compose -f docker-compose.prod.yml logs -f
```

容器内置 healthcheck（打 `https://localhost:5443/api/readiness`），用
`docker ps` 或 `docker inspect --format '{{.State.Health.Status}}' jiamiyunpan`
查看健康状态。

**备份与恢复**见 [docs/BACKUP.md](./docs/BACKUP.md) —— 升级前请先备份
`sqlite.db`：SeaORM migration 没有 downgrade，回滚旧版本前必须有 DB 备份。

## 配置

完整变量表见 [`.env.example`](./.env.example)。

### 核心

| 变量 | 默认 | 说明 |
|------|------|------|
| `DATA_DIR` | 必填 | 数据库与本地块文件目录 |
| `DATABASE_URL` | SQLite | 填了就走 PostgreSQL |
| `APP_URL` | `https://localhost:5443` | 对外访问地址 |
| `APP_CLIENT_URL` | 同 `APP_URL` | 前端地址，开发时指向 Vite |
| `HTTP_PORT` | `5443` | 监听端口 |
| `HTTP_ADDRESS` | `localhost` | 绑定地址，容器里用 `0.0.0.0` |

SQLite 与 PostgreSQL 数据不通用，写入后不能来回切。

### TLS

| 变量 | 默认 | 说明 |
|------|------|------|
| `SSL_DISABLED` | `false` | 关闭 TLS，仅开发用 |
| `SSL_CERT_FILE` | `DATA_DIR/jiamiyunpan.crt.pem` | 证书路径，缺失则自签 |
| `SSL_KEY_FILE` | `DATA_DIR/jiamiyunpan.key.pem` | 私钥路径，缺失则自签 |

### 会话

| 变量 | 默认 | 说明 |
|------|------|------|
| `JWT_SECRET` | 随机 | JWT 签名密钥，务必固定 |
| `LONG_TERM_SESSION_DURATION_DAYS` | `30` | 空闲会话保留天数 |
| `SHORT_TERM_SESSION_DURATION_SECONDS` | `120` | 短令牌有效期 |
| `SESSION_COOKIE` | `jiamiyunpan_session` | 会话 Cookie 名 |
| `REFRESH_COOKIE` | `jiamiyunpan_refresh` | 刷新令牌 Cookie 名 |
| `COOKIE_HTTP_ONLY` | `true` | 禁止 JS 读取会话 Cookie |
| `COOKIE_SECURE` | `true` | 仅 HTTPS 发送 |
| `COOKIE_SAME_SITE` | `Lax` | `Lax` / `Strict` / `None` |
| `COOKIE_DOMAIN` | 取自 `APP_URL` | 需要时手动覆盖 |

前后端不同域、或要给独立 App 调 API 时，用 `USE_HEADERS_FOR_AUTH=true` 改走响应头 + localStorage。
这会引入 XSS 暴露风险，同域部署保持默认 `false`。

### 邮件

`MAILER_TYPE=none`（默认）时账户自动激活、不发信。改 `smtp` 后启用邮箱验证与分享通知。

| 变量 | 默认 | 说明 |
|------|------|------|
| `MAILER_TYPE` | `none` | `smtp` / `none` |
| `SMTP_ADDRESS` | | SMTP 主机 |
| `SMTP_USERNAME` | | 登录名 |
| `SMTP_PASSWORD` | | 密码 |
| `SMTP_PORT` | `465` | 端口，未指定 `SMTP_TLS_MODE` 时按端口推断 |
| `SMTP_TLS_MODE` | 自动 | `implicit` / `starttls` / `none` |
| `SMTP_DEFAULT_FROM_EMAIL` | | 发件地址 |
| `SMTP_DEFAULT_FROM_NAME` | | 发件显示名 |

### 存储

默认本地磁盘，`STORAGE_PROVIDER=s3` 切对象存储。

| 变量 | 默认 | 说明 |
|------|------|------|
| `STORAGE_PROVIDER` | `local` | `local` / `s3` |
| `TAR_TRANSFER_DISABLED` | `false` | 禁用 tar 批量传输，反代限包体时用 |
| `S3_BUCKET` | | 桶名 |
| `S3_REGION` | `us-east-1` | 区域 |
| `S3_ENDPOINT` | AWS 默认 | MinIO / B2 / Wasabi 等自定义端点 |
| `S3_ACCESS_KEY` | | Access Key |
| `S3_SECRET_KEY` | | Secret Key |
| `S3_PATH_STYLE` | `false` | MinIO 需 `true` |
| `S3_PREFIX` | | 共享桶内的对象前缀 |
| `S3_DIRECT_TRANSFER` | `false` | 客户端直连桶 |
| `S3_DIRECT_EXPIRY_SECS` | `604800` | 预签名 URL 有效期，上限 7 天 |
| `S3_DIRECT_ALLOW_INSECURE` | `false` | 跳过 HTTPS / 证书 / 内网地址校验 |

> 用 S3 时 `DATA_DIR` 仍然必填，数据库和本地状态还在里面，只有密文块进桶。

### 直连模式要求

开启 `S3_DIRECT_TRANSFER=true` 后，桶必须**客户端可达**，不只是服务端可达：

| 要求 | 原因 |
|------|------|
| 客户端能解析并访问 | 签名 URL 指向只有服务器能到的地址等于没用 |
| HTTPS | HTTPS 页面不能拉 HTTP 资源，用户无法绕过 |
| 公信证书 | 浏览器不信任你的内网 CA |
| CORS 放行 `APP_URL` 的 `GET`/`PUT` | 否则浏览器丢弃响应 |
| 服务端与客户端用同一主机名 | 签名覆盖 host |

启动时会做真实 CORS 预检校验，失败就自动退回中转模式并打日志。状态可查：

```shell
curl -s https://drive.example.com/api/readiness
```

### 本地存储迁移到 S3

> 迁移前先停服务，否则正在上传的块可能漏迁。

```shell
# 1. 先在 compose 里补 S3 变量，STORAGE_PROVIDER 仍保持 local
# 2. 执行迁移（幂等，可重跑）
docker exec jiamiyunpan jiamiyunpan migrate-storage

# 3. 切 STORAGE_PROVIDER=s3 后重启
docker compose up -d
```

## 开发

见 [DEVELOPMENT.md](./DEVELOPMENT.md)：工具链、`just` 命令、测试与 CI。

```shell
just dev        # 前端 Vite + 后端 bacon 热重载
just test       # Rust + 前端单测
just wasm       # 构建 cryptfns 的 WASM 包
```

## 品牌定制

项目里的品牌名集中在少数位置，换名字只需要改这些：

| 位置 | 变量 |
|------|------|
| 后端显示名 | `APP_NAME`（默认 `甲米云盘`） |
| 前端标题 | `web/index.html` 的 `<title>` 与 `meta description` |
| 界面文案 | `web/src/locales/*.json` |
| 图标 | `web/public/*` 与 `web/index.html` 的 icon 列表 |
| 证书与 Cookie | `SSL_CERT_FILE` / `SESSION_COOKIE` 等环境变量 |

若要把内部协议标识也一起换掉，需要同时改 Rust 与 TS 两侧的 domain separator
（`cryptfns/src/asn1.rs` 与 `web/services/shares/crypto.ts`），并且只能对全新部署这么做 ——
改了之后老数据无法验签。

## 许可证

本项目基于 **Hoodik**（作者 Tibor Hudik）二次开发，遵循
[CC BY-NC 4.0](./LICENSE.md)：允许复制、修改、非商业使用，**必须保留署名**；
商业用途需另行取得授权。

## 版权与免责声明

**版权与署名**

- 本项目的原始代码 **Hoodik** 版权归 **Tibor Hudík（htunlogic）** 所有，本项目依据
  [CC BY-NC 4.0](./LICENSE.md) 许可进行修改与再分发，并按要求保留原作者署名。
- 二次开发的修改部分版权归本仓库维护者所有，同样以 CC BY-NC 4.0 提供。
- "甲米云盘 / Jiami Drive / jiamiyunpan" 名称与图标为维护者自有标识；与 Hoodik、
  以及任何其他同名或近名的商业产品、公司、商标**均无任何隶属、合作或背书关系**。

**免责声明（请完整阅读后再使用）**

1. **无担保**：本项目按"现状"（AS IS）提供，不附带任何明示或暗示的担保，包括但不限于
   适销性、特定用途适用性与不侵权的担保。作者与贡献者不对任何直接、间接、附带、
   特殊或后果性损害承担责任。
2. **数据安全自担风险**：虽然本项目实现了端到端加密，但任何软件都无法保证绝对安全。
   使用者须自行负责密钥保管、备份策略、服务器与依赖的安全加固；密钥丢失、数据损坏、
   未授权访问等造成的任何损失由使用者自行承担。
3. **非商业用途限制**：CC BY-NC 4.0 禁止将本项目及其修改版用于商业目的。任何商业使用
   需同时取得原作者与本项目维护者的书面授权。
4. **合规自负**：本项目为自托管软件，部署者需自行确保其使用方式符合所在国家或地区的
   法律法规（包括但不限于数据存储、内容合规与网络安全相关规定）；因违规使用产生的
   一切法律后果由部署者承担。
5. **第三方组件**：本项目依赖的第三方库各自遵循其原始许可证，详见各依赖的声明；
   本项目声明不代表任何第三方组件的作者立场。

---

维护者：btc3149-bit
