---
name: tauri-ssh-app-deploy
description: 通用「用 Tauri SSH 部署任意应用」配方（框架无关）—— 共享中间件 镜像商店安装 + 部署目标 upsert_deployment_target(compose 托管)，产物全进工作台记录、可一键重部署/回滚。任何项目部署没有专属框架技能时的默认剧本。
触发词: 部署, 上线, 发布, deploy, 部署到, 部署项目, 部署应用, 部署后端, 部署前端, docker部署, docker 部署, 容器部署, 上服务器, 发布到服务器, 部署到服务器, 生产部署, 上生产, 全栈部署, 反向代理部署, 用tauri-ssh部署, tauri-ssh部署
---

# 用 Tauri SSH 部署任意应用（通用配方 · 框架无关）

适用：用户要把**任意项目**（Spring Boot / Node / Python / Go / PHP / 静态站 …）部署到远程服务器。
**先查有没有针对该框架的专属技能**（`evaluate_skills` 命中如 `ruoyi-plus-uniapp-deploy` 就优先用它，更精确）；没有就用本通用剧本。

> 🔴 **核心纪律：一切走「Tauri SSH 工具 + 部署目标(Target)」，绝不用 `ssh_exec` 手写 docker-compose / openssl 生成 .env / 手动建库账号**。
> 手搓的产物**不进任何记录**（数据库 / Redis / 安全凭证 / 网站 / 证书 / 部署历史全空）、容器命名乱、用户**不能在工作台查看、不能一键重部署 / 回滚**——这是反复踩过的坑。

## 术语：`<应用标识>`（贯穿全程的唯一名）
取应用自身配置里的唯一标识，**全程一致**用它命名（app 容器名 / 数据库名 / per-app 库账号 / 部署目标名 / 栈目录）：
- Spring Boot：`application.yml` 的 `app.id` 或 datasource 库名；
- Node：`package.json` 的 `name`；其它框架同理取项目唯一名；都没有 → 用项目目录名（小写、`[a-z0-9_]`）。
> 例：ruoyi 的 `<应用标识>` = `ryplus_uni`。下文 `<应用标识>` 处一律替换成它。

## 🤖 第零步：优先用 Tauri SSH 专用工具（别裸 SSH 即兴）
- **探环境**（只读，任何档位放行）：`system_info` / `disk_usage`（内存+swap、磁盘）、`port_check`、`service_status`、`ssh_exec`(只读) 查 Docker / 域名解析。
- **装共享中间件**：`install_deployment_image_store_app`（自动部署 → 镜像商店）（镜像商店同款：密码 Tauri SSH 生成同步进容器+安全凭证库**必连得上**、容器 `tauri-ssh-<app>`、绑 127.0.0.1、生成对应数据库管理连接）。🔴 **别用 `自定义部署脚本` 手写脚本装这些库**（手写易致"存的密码≠容器密码"、工作台连不上）。
- **建/触发部署**：`upsert_deployment_target`（产出部署目标，只写不执行）→ `create_deployment_dry_run`（预览）→ `execute_deployment_run`（执行，进部署历史、可回滚）。
- **传产物**：`sftp_upload`（jar / dist 等大文件）；`sftp_read`/`sftp_write`（小文件 / 改配置）。
- 改动型（装库 / 部署 / sudo）按服务器档位走**审批**——提前告知用户、被拒别原样重试。

## 1. 探环境 → 报告 → 等确认（严禁探完直接干）
探完把现状 + **默认 Docker 方案**讲给用户，**拿到明确「确认 / 继续」才动手**。重点报：系统 / **内存+swap**（<2G 全栈必加 swap）/ 磁盘、Docker 装没装、域名解析、80/443 安全组、本地构建链。

## 2. 共享中间件一次装好（通用设施，整机一套）
- 需要 MySQL/Redis/PostgreSQL 等 → 用 **`install_deployment_image_store_app`** 装（见第零步）。
- 🔴 **通用设施，不带项目名**：数据库连接 label = 镜像商店规范名 `MySQL` / `Redis`（user=root / db=0），容器 `tauri-ssh-mysql`/`tauri-ssh-redis`。**已装就复用**（`list_database_connections` 查），第二个项目共用同一套、**别重复装、别套项目前缀**。
- 记下生成的数据库连接 key，下一步在 `serviceAccounts` 中引用。

## 3. 建后端部署目标 `upsert_deployment_target`（recipe=`docker-compose`，开 `compose` 托管）
- `name`: `<应用标识>-backend`（target 名可带端后缀，不影响凭据 / 栈名）；`servers`: `["<别名>"]`。
- 🔴 **`release.deployRoot` 必须 = `/opt/tauri-ssh/stacks/<应用标识>`**：compose 落这里、compose project = `<应用标识>`（唯一）→ Tauri SSH「容器/编排」页认作**自己的栈**（有启停/重建/编辑按钮）。**别用** `/opt/<app>` 等 stacks 外目录（会被扫成「外部栈」只读、project 名取目录名易撞）。
- **`compose.template`：只含 app 一个服务**——🔴 不在里面再起 mysql/redis（用第 2 步的共享库）。**容器名 = `<应用标识>` 本身**（🔴 不加 `-app` 等后缀）；密钥写哨兵 `__DB_PASSWORD__` / `__REDIS_PASSWORD__` / `__JWT_SECRET_KEY__`（**禁明文**）。
- 🔴 **app↔共享库接线（最易错）**：共享库绑宿主 `127.0.0.1`、**不在** app 的 compose 网络里——`DB_HOST=mysql` 那套桥接网 DNS **连不到**。正解：app 走 **host 网络**（`network_mode: host`）、env `DB_HOST=127.0.0.1`/`REDIS_HOST=127.0.0.1`（不是服务名）；或 `extra_hosts: ["host.docker.internal:host-gateway"]` + `DB_HOST=host.docker.internal`。
- 🔴 **per-app 库+账号必须靠 `serviceAccounts.database` 让引擎建，绝不手动**：`serviceAccounts.database`: `{ enabled:true, connectionKey:"<共享 MySQL 连接>", databaseName:"<应用标识>", username:"<应用标识>", credentialKey:"<应用标识>_mysql_app" }` → 引擎自动「建库+建独立账号+**登记成第二条独立 mysql_conn(label=<应用标识>)**」。
  - ⛔ **禁止** `ssh_exec`/`db_execute` 跑 `CREATE DATABASE`/`CREATE USER`/`GRANT` 自己建——账号建了但**不登记成凭据**（安全凭证页缺 app 专属那条、最小权限连接没法用）。
  - ✅ **终态 = 两条数据库连接**：① 共享 `MySQL`(root) ② app 专属 `<应用标识>`；app 用 ②（最小权限）连库，① 仅供 Tauri SSH 管理建库。
- 用 Redis → `serviceAccounts.redis`: `{ enabled:true, connectionKey:"<共享 Redis 连接>", databaseName:"0", credentialKey:"<应用标识>_redis_app" }`；自动生成的密钥放 `secrets`（如 `["JWT_SECRET_KEY"]`）。
- 首次要导 SQL → `initSqls`: `["<本机绝对路径>/xxx.sql", ...]`（按序）。
- 🔴 **后端 target 不要设 `domain`**（后端只监听 `127.0.0.1:<端口>`）——域名 / HTTPS / 同域 API 反代统一由第 4 步前端 target 接管。给后端设 domain 会触发**整域反代到后端**、把前端 SPA 挤掉。
- 产物：本机构建（如 jar / 镜像）由 target 的 `build_commands` / `image` / `artifact` 承载；运行时 / 内存调优按框架定（低内存：swap + `mem_limit` + 限堆，**仍 Docker**）。

## 4. 预览 → 等确认 → 执行
`create_deployment_dry_run` 看各阶段命令 → 讲给用户、**等确认** → `execute_deployment_run`。之后用户在「部署」页一键重部署 / 回滚。

## 5. 前端（若有 SPA / 静态站）= 一个 static-openresty 部署目标（域名 / HTTPS / 同域 API 反代一站搞定，**别手写 vhost**）
本机构建静态产物（前端 `.env` 的 API base 要与下面 `apiProxyPrefix` 一致）。然后 `upsert_deployment_target` 建前端目标（recipe=`static-openresty`，buildSource=`artifact`）：
- `name`: `<应用标识>-web`；`servers`: `["<别名>"]`；`domain`: `<域名>`；`https`: `true`；`artifact.localDir`: 前端 `dist`。
- `vars`: `{ "apiProxyPort": "<后端端口>", "apiProxyPrefix": "<API 前缀>", "sslEmail": "<邮箱>" }`
  → 引擎自动：建静态站（`/`→SPA）+ **注入 `location /<前缀>/ → 127.0.0.1:<后端端口>`**（含 WebSocket 头，80/443 都注入）+ 签 Let's Encrypt 证书。
- **同域名「前端 SPA + 后端 API + HTTPS」一次完成，无需手写 vhost、无需自己调 `自动部署网站能力`。**
- 纯后端 / 无前端 SPA → 不建前端 target；要给后端单独配域名+HTTPS，用 `自动部署反向代理目标` + `自动部署 HTTPS 证书`（域名 → `127.0.0.1:<端口>`）。

## 6. 验收（用真实接口，别用探活端点）
请求一个**真实公开接口**（登录页会调的验证码 / 配置 / 健康业务接口），HTTPS 返回 200 + 正常内容 → 证明 HTTPS→反代→后端→DB/Redis 整链路通。**别**只依赖 `/actuator/health` 这类（很多项目关了会 404 误判）。浏览器再看页面渲染 + 无控制台报错。

## 收尾自检（缺啥补啥，确保工作台可见可管）
- 打开 Tauri SSH 工作台，**数据库 / 安全凭证 / 网站 / 证书 / 容器(编排) / 部署历史** 里都能看到这次部署吗？不能则补登记。
- 🔴 **用了 MySQL → 数据库管理连接应有共享连接和应用专属连接**：共享 `MySQL`(root) + app 专属 `<应用标识>`。只有一条 = 没配置 `serviceAccounts.database`（漏登记）→ 回到部署目标配置 `serviceAccounts.database` 后重新 dry-run/执行，让后端按规范创建和保存。
- 🔴 **后端容器在「编排」页是「自己的栈」而非「外部」** → 若显示「外部」=deployRoot 没落在 `/opt/tauri-ssh/stacks/`，下次部署改对。

## 铁律
- 默认 Docker；内存紧张 = swap + `mem_limit` + 限堆 / 调 DB 缓冲，**仍 Docker**，不擅自切原生（除非用户明确要）。
- 共享 MySQL/Redis 通用名、整机一套；app 自身的东西（容器 / 库 / 库账号 / 栈目录 / target）一律用 `<应用标识>`。
- 凭据永不进对话 / 日志（`[REDACTED:xxx]` 是正常脱敏，别因此重新生成密码死循环）。
- 产物一律进记录（走镜像商店、upsert_deployment_target、数据库管理连接和自动部署网站能力），别手搓绕过。
