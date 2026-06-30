import { hasTauriRuntime, invoke } from "./client";
import type {
  CreateDeploymentDryRunInput,
  CreateDeploymentRollbackDryRunInput,
  DeploymentAiAdviceInput,
  DeploymentAiAdviceResult,
  DeploymentDetectionResult,
  DeploymentEnvironmentProfile,
  DeploymentGroup,
  DeploymentImageStoreApp,
  DeploymentPlan,
  DeploymentRun,
  DeploymentRunDetail,
  DeploymentTarget,
  DeploymentTemplate,
  DetectDeploymentProjectInput,
  ExecuteDeploymentRollbackInput,
  ExecuteDeploymentRunInput,
  InstallImageStoreAppInput,
  ListDeploymentRunsInput,
  UpsertDeploymentGroupInput,
  UpsertDeploymentTargetInput,
} from "@/types";

const fallbackTemplates: DeploymentTemplate[] = [
  {
    key: "1panel-app",
    name: "1panel-app",
    description: "1Panel 托管应用部署，按 1Panel 目录约定上传产物并重启对应 compose 服务。",
    scenario: "1Panel 托管应用",
    risk: "high",
    supportedSources: ["local", "git"],
    requiredProfiles: ["1panel", "docker"],
  },
  {
    key: "dockerfile-service",
    name: "Dockerfile 服务",
    description: "识别 Dockerfile，支持远程构建或本地构建镜像后上传。",
    scenario: "单服务容器部署",
    risk: "review",
    supportedSources: ["local", "git"],
    requiredProfiles: ["dockerfile-service"],
  },
  {
    key: "docker-compose",
    name: "Docker Compose 栈",
    description: "识别 compose 文件，将栈托管到部署根目录。",
    scenario: "多容器编排",
    risk: "review",
    supportedSources: ["local", "git"],
    requiredProfiles: ["docker-compose"],
  },
  {
    key: "static-openresty",
    name: "前端静态站",
    description: "构建前端产物，上传静态资源，并预留 HTTPS 和 API 反代配置。",
    scenario: "React/Vue/Vite/Uniapp 静态站",
    risk: "review",
    supportedSources: ["local", "git"],
    requiredProfiles: ["static-openresty"],
  },
  {
    key: "static-nginx",
    name: "Nginx 静态站",
    description: "前端静态站部署到 nginx，使用 releases 软链原子切换并 reload nginx。",
    scenario: "React/Vue/Vite/Uniapp 静态站",
    risk: "review",
    supportedSources: ["local", "git"],
    requiredProfiles: ["static-nginx"],
  },
  {
    key: "node-pm2",
    name: "Node PM2 服务",
    description: "Node 后端服务上传 release 后由 PM2 托管。",
    scenario: "Node API 服务",
    risk: "high",
    supportedSources: ["local", "git"],
    requiredProfiles: ["node-pm2"],
  },
  {
    key: "systemd-binary",
    name: "Systemd 二进制服务",
    description: "Java/Go/二进制产物上传后由 systemd 托管。",
    scenario: "JAR、Go、二进制服务",
    risk: "high",
    supportedSources: ["local", "git"],
    requiredProfiles: ["systemd-binary"],
  },
  {
    key: "custom-script",
    name: "自定义脚本",
    description: "兜底部署方案，所有命令强制 dry-run、危险命令扫描和审批。",
    scenario: "非标准项目",
    risk: "high",
    supportedSources: ["local", "git"],
    requiredProfiles: [],
  },
];

const fallbackProfiles: DeploymentEnvironmentProfile[] = [
  {
    key: "1panel-app",
    name: "1panel-app",
    description: "1panel 托管应用部署（按 1panel 目录约定上传产物 + 重启对应 compose/服务）。",
    category: "基础模板",
    checks: ["1panel", "docker"],
  },
  {
    key: "custom-script",
    name: "custom-script",
    description: "兜底万能配方，各阶段命令全部自定义（artifact 模式仍走 releases 软链原子切换）。",
    category: "基础模板",
    checks: ["custom"],
  },
  {
    key: "docker-compose",
    name: "docker-compose",
    description: "Docker Compose 部署（拉镜像 / 重建容器；后端反代 + HTTPS 由部署引擎统一接管，配了域名即自动配反代）。",
    category: "基础模板",
    checks: ["docker"],
  },
  {
    key: "node-pm2",
    name: "node-pm2",
    description: "Node 后端用 pm2 部署（releases + 软链原子切换，pm2 reload 平滑重启）。",
    category: "基础模板",
    checks: ["node", "backend"],
  },
  {
    key: "static-nginx",
    name: "static-nginx",
    description: "前端静态站部署到 nginx（releases + 软链原子切换，reload nginx）。",
    category: "基础模板",
    checks: ["static", "frontend"],
  },
  {
    key: "static-openresty",
    name: "static-openresty",
    description: "前端静态站部署到 OpenResty（releases + current 软链原子切换；建站 + HTTPS 由部署引擎复用「网站」系统统一接管，不手写 conf）。",
    category: "基础模板",
    checks: ["static", "frontend"],
  },
  {
    key: "systemd-binary",
    name: "systemd-binary",
    description: "Java jar / Go / 二进制用 systemd 部署（releases + 软链原子切换，systemctl restart）。",
    category: "基础模板",
    checks: ["java", "go", "binary", "backend"],
  },
  {
    key: "static-openresty-https",
    name: "前端静态站 + HTTPS",
    description: "适合 Vite/React/Vue/Uniapp 等纯前端项目，默认使用 OpenResty 静态站、80 端口健康检查，并预留域名 HTTPS 配置。",
    category: "组合方案",
    checks: ["static", "openresty", "https"],
  },
  {
    key: "springboot-mysql-redis",
    name: "Spring Boot + MySQL + Redis",
    description: "适合 Java 后端服务，默认使用 systemd 托管，并在扩展配置中预置 MySQL/Redis 专属账号创建结构。",
    category: "组合方案",
    checks: ["java", "systemd", "mysql", "redis"],
  },
  {
    key: "compose-db-redis",
    name: "Docker Compose + 数据库/Redis",
    description: "适合多容器应用复用宿主共享 MySQL/Redis，默认使用 Compose 配方，并预置数据库和 Redis 专属账号配置。",
    category: "组合方案",
    checks: ["docker", "compose", "mysql", "redis"],
  },
  {
    key: "frontend-api-same-domain",
    name: "前后端同域部署",
    description: "适合 SPA 前端和后端 API 同域发布，默认使用 OpenResty 静态站并预置 API 反代前缀和后端端口配置。",
    category: "组合方案",
    checks: ["static", "openresty", "api-proxy", "https"],
  },
  {
    key: "1panel-app-db",
    name: "1Panel 应用 + 共享数据库",
    description: "适合 1Panel 托管应用复用应用内数据库/Redis 资源，默认使用 1Panel 配方并预置专属账号结构。",
    category: "组合方案",
    checks: ["1panel", "docker", "mysql", "redis"],
  },
];

const fallbackImageStoreApps: DeploymentImageStoreApp[] = [
  {
    key: "nginx",
    name: "Nginx",
    description: "Web 服务 / 静态文件服务",
    category: "常用镜像",
    image: "nginx",
    tag: "latest",
    defaultPort: 8080,
    containerPort: 80,
    volumePath: "/usr/share/nginx/html",
    env: [],
    notes: ["默认映射到宿主 8080 端口，可后续挂载站点目录。"],
  },
  {
    key: "mysql",
    name: "MySQL",
    description: "关系型数据库",
    category: "常用镜像",
    image: "mysql",
    tag: "8.4",
    defaultPort: 3306,
    containerPort: 3306,
    volumePath: "/var/lib/mysql",
    env: [{ key: "MYSQL_ROOT_PASSWORD", label: "Root 密码", defaultValue: "ChangeMe_123456", required: true, secret: true }],
    notes: ["生产环境请修改默认 Root 密码。"],
  },
  {
    key: "postgres",
    name: "PostgreSQL",
    description: "关系型数据库",
    category: "常用镜像",
    image: "postgres",
    tag: "16",
    defaultPort: 5432,
    containerPort: 5432,
    volumePath: "/var/lib/postgresql/data",
    env: [
      { key: "POSTGRES_USER", label: "用户名", defaultValue: "postgres", required: true, secret: false },
      { key: "POSTGRES_PASSWORD", label: "密码", defaultValue: "ChangeMe_123456", required: true, secret: true },
    ],
    notes: ["生产环境请修改默认数据库密码。"],
  },
  {
    key: "redis",
    name: "Redis",
    description: "缓存 / KV 存储",
    category: "常用镜像",
    image: "redis",
    tag: "7",
    defaultPort: 6379,
    containerPort: 6379,
    volumePath: "/data",
    env: [],
    notes: ["默认未开启密码，生产环境建议在配置中补充 requirepass。"],
  },
  {
    key: "portainer",
    name: "Portainer",
    description: "Docker 可视化管理",
    category: "常用镜像",
    image: "portainer/portainer-ce",
    tag: "latest",
    defaultPort: 9000,
    containerPort: 9000,
    volumePath: "/data",
    env: [],
    notes: ["会挂载 /var/run/docker.sock，请仅安装在可信服务器。"],
  },
  {
    key: "minio",
    name: "MinIO",
    description: "S3 兼容对象存储",
    category: "常用镜像",
    image: "minio/minio",
    tag: "latest",
    defaultPort: 9001,
    containerPort: 9001,
    volumePath: "/data",
    env: [
      { key: "MINIO_ROOT_USER", label: "Root 用户", defaultValue: "minioadmin", required: true, secret: false },
      { key: "MINIO_ROOT_PASSWORD", label: "Root 密码", defaultValue: "ChangeMe_123456", required: true, secret: true },
    ],
    notes: ["API 默认容器端口 9000，控制台默认映射到宿主 9001。"],
  },
  {
    key: "gitea",
    name: "Gitea",
    description: "轻量 Git 服务",
    category: "常用镜像",
    image: "gitea/gitea",
    tag: "latest",
    defaultPort: 3000,
    containerPort: 3000,
    volumePath: "/data",
    env: [],
    notes: ["SSH 端口未默认映射，可在配置 JSON 中扩展 compose。"],
  },
  {
    key: "grafana",
    name: "Grafana",
    description: "监控可视化",
    category: "常用镜像",
    image: "grafana/grafana",
    tag: "latest",
    defaultPort: 3001,
    containerPort: 3000,
    volumePath: "/var/lib/grafana",
    env: [
      { key: "GF_SECURITY_ADMIN_USER", label: "管理员", defaultValue: "admin", required: true, secret: false },
      { key: "GF_SECURITY_ADMIN_PASSWORD", label: "管理员密码", defaultValue: "ChangeMe_123456", required: true, secret: true },
    ],
    notes: ["Grafana 容器端口是 3000，默认映射到宿主 3001。"],
  },
  {
    key: "rabbitmq",
    name: "RabbitMQ",
    description: "消息队列",
    category: "常用镜像",
    image: "rabbitmq",
    tag: "3-management",
    defaultPort: 15672,
    containerPort: 15672,
    volumePath: "/var/lib/rabbitmq",
    env: [],
    notes: ["AMQP 5672 未默认作为主端口展示，可在 compose 中扩展映射。"],
  },
  {
    key: "mongo",
    name: "MongoDB",
    description: "文档数据库",
    category: "常用镜像",
    image: "mongo",
    tag: "7",
    defaultPort: 27017,
    containerPort: 27017,
    volumePath: "/data/db",
    env: [],
    notes: ["默认未启用认证，生产环境请补充账号密码配置。"],
  },
  {
    key: "rocketmq-namesrv",
    name: "RocketMQ NameServer",
    description: "RocketMQ 注册中心",
    category: "常用镜像",
    image: "apache/rocketmq",
    tag: "5.3.0",
    defaultPort: 9876,
    containerPort: 9876,
    volumePath: "/home/rocketmq/logs",
    env: [{ key: "JAVA_OPT_EXT", label: "JVM 参数", defaultValue: "-server -Xms256m -Xmx256m", required: false, secret: false }],
    notes: ["默认启动 NameServer；Broker 请使用 RocketMQ Broker 镜像项单独安装。"],
  },
  {
    key: "rocketmq-broker",
    name: "RocketMQ Broker",
    description: "RocketMQ 消息 Broker",
    category: "常用镜像",
    image: "apache/rocketmq",
    tag: "5.3.0",
    defaultPort: 10911,
    containerPort: 10911,
    volumePath: "/home/rocketmq/store",
    env: [
      { key: "NAMESRV_ADDR", label: "NameServer 地址", defaultValue: "127.0.0.1:9876", required: true, secret: false },
      { key: "JAVA_OPT_EXT", label: "JVM 参数", defaultValue: "-server -Xms512m -Xmx512m", required: false, secret: false },
    ],
    notes: ["默认按单机快速部署生成；生产环境请把 NAMESRV_ADDR 改为真实 NameServer 地址。"],
  },
  {
    key: "elasticsearch",
    name: "Elasticsearch",
    description: "搜索引擎 / 日志检索",
    category: "常用镜像",
    image: "docker.elastic.co/elasticsearch/elasticsearch",
    tag: "8.15.3",
    defaultPort: 9200,
    containerPort: 9200,
    volumePath: "/usr/share/elasticsearch/data",
    env: [
      { key: "discovery.type", label: "发现模式", defaultValue: "single-node", required: true, secret: false },
      { key: "xpack.security.enabled", label: "安全认证", defaultValue: "false", required: true, secret: false },
      { key: "ES_JAVA_OPTS", label: "JVM 参数", defaultValue: "-Xms512m -Xmx512m", required: false, secret: false },
    ],
    notes: ["生产环境建议开启认证，并提前设置 vm.max_map_count。"],
  },
  {
    key: "skywalking-oap",
    name: "SkyWalking OAP",
    description: "APM 后端分析服务",
    category: "常用镜像",
    image: "apache/skywalking-oap-server",
    tag: "10.0.1",
    defaultPort: 12800,
    containerPort: 12800,
    volumePath: "/skywalking/ext-config",
    env: [{ key: "SW_STORAGE", label: "存储类型", defaultValue: "h2", required: true, secret: false }],
    notes: ["默认使用 H2 便于快速体验；生产环境建议切换到 Elasticsearch 存储。"],
  },
  {
    key: "skywalking-ui",
    name: "SkyWalking UI",
    description: "APM 可视化界面",
    category: "常用镜像",
    image: "apache/skywalking-ui",
    tag: "10.0.1",
    defaultPort: 8080,
    containerPort: 8080,
    volumePath: "/skywalking/ext-config",
    env: [{ key: "SW_OAP_ADDRESS", label: "OAP 地址", defaultValue: "http://127.0.0.1:12800", required: true, secret: false }],
    notes: ["如果 OAP 不在同一宿主机，请修改 SW_OAP_ADDRESS。"],
  },
  {
    key: "elk",
    name: "ELK",
    description: "Elasticsearch + Logstash + Kibana",
    category: "常用镜像",
    image: "docker.elastic.co/elasticsearch/elasticsearch",
    tag: "8.15.3",
    defaultPort: 5601,
    containerPort: 5601,
    volumePath: "/usr/share/elasticsearch/data",
    env: [
      { key: "ELASTIC_PASSWORD", label: "Elastic 密码", defaultValue: "ChangeMe_123456", required: false, secret: true },
      { key: "ES_JAVA_OPTS", label: "ES JVM 参数", defaultValue: "-Xms512m -Xmx512m", required: false, secret: false },
    ],
    notes: ["会生成三容器 compose，默认开放 Kibana 5601、Elasticsearch 9200、Logstash 5044。"],
  },
];

function requireTauriRuntime(): never {
  throw new Error("当前浏览器预览环境无法调用本地文件检测，请在 Tauri 桌面端使用该功能。");
}

export const deploymentApi = {
  listTemplates: () =>
    hasTauriRuntime()
      ? invoke<DeploymentTemplate[]>("list_deployment_templates")
      : Promise.resolve(fallbackTemplates),
  listEnvironmentProfiles: () =>
    hasTauriRuntime()
      ? invoke<DeploymentEnvironmentProfile[]>("list_deployment_environment_profiles")
      : Promise.resolve(fallbackProfiles),
  listImageStoreApps: () =>
    hasTauriRuntime()
      ? invoke<DeploymentImageStoreApp[]>("list_deployment_image_store_apps")
      : Promise.resolve(fallbackImageStoreApps),
  installImageStoreApp: (input: InstallImageStoreAppInput) =>
    hasTauriRuntime()
      ? invoke<DeploymentTarget>("install_deployment_image_store_app", { input })
      : Promise.resolve(requireTauriRuntime()),
  detectProject: (input: DetectDeploymentProjectInput) =>
    hasTauriRuntime()
      ? invoke<DeploymentDetectionResult>("detect_deployment_project", { input })
      : Promise.resolve(requireTauriRuntime()),
  listTargets: () =>
    hasTauriRuntime()
      ? invoke<DeploymentTarget[]>("list_deployment_targets")
      : Promise.resolve([] as DeploymentTarget[]),
  upsertTarget: (input: UpsertDeploymentTargetInput) =>
    hasTauriRuntime()
      ? invoke<DeploymentTarget>("upsert_deployment_target", { input })
      : Promise.resolve(requireTauriRuntime()),
  deleteTarget: (targetKey: string) =>
    hasTauriRuntime()
      ? invoke<void>("delete_deployment_target", { targetKey })
      : Promise.resolve(requireTauriRuntime()),
  listGroups: () =>
    hasTauriRuntime()
      ? invoke<DeploymentGroup[]>("list_deployment_groups")
      : Promise.resolve([] as DeploymentGroup[]),
  upsertGroup: (input: UpsertDeploymentGroupInput) =>
    hasTauriRuntime()
      ? invoke<DeploymentGroup>("upsert_deployment_group", { input })
      : Promise.resolve(requireTauriRuntime()),
  deleteGroup: (groupKey: string) =>
    hasTauriRuntime()
      ? invoke<void>("delete_deployment_group", { groupKey })
      : Promise.resolve(requireTauriRuntime()),
  createDryRun: (input: CreateDeploymentDryRunInput) =>
    hasTauriRuntime()
      ? invoke<DeploymentPlan>("create_deployment_dry_run", { input })
      : Promise.resolve(requireTauriRuntime()),
  executeRun: (input: ExecuteDeploymentRunInput) =>
    hasTauriRuntime()
      ? invoke<DeploymentRunDetail>("execute_deployment_run", { input })
      : Promise.resolve(requireTauriRuntime()),
  listRuns: (input: ListDeploymentRunsInput = { status: "all", limit: 50 }) =>
    hasTauriRuntime()
      ? invoke<DeploymentRun[]>("list_deployment_runs", { input })
      : Promise.resolve([] as DeploymentRun[]),
  getRunDetail: (runId: string) =>
    hasTauriRuntime()
      ? invoke<DeploymentRunDetail>("get_deployment_run_detail", { runId })
      : Promise.resolve(requireTauriRuntime()),
  createRollbackDryRun: (input: CreateDeploymentRollbackDryRunInput) =>
    hasTauriRuntime()
      ? invoke<DeploymentPlan>("create_deployment_rollback_dry_run", { input })
      : Promise.resolve(requireTauriRuntime()),
  executeRollback: (input: ExecuteDeploymentRollbackInput) =>
    hasTauriRuntime()
      ? invoke<DeploymentRunDetail>("execute_deployment_rollback", { input })
      : Promise.resolve(requireTauriRuntime()),
  askAiAdvice: (input: DeploymentAiAdviceInput) =>
    hasTauriRuntime()
      ? invoke<DeploymentAiAdviceResult>("ask_deployment_ai_advice", { input })
      : Promise.resolve(requireTauriRuntime()),
};
