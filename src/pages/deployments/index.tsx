import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Alert,
  Button,
  Card,
  Collapse,
  Descriptions,
  Drawer,
  Form,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Select,
  Space,
  Statistic,
  Switch,
  Table,
  Tabs,
  Tag,
  Typography,
  message,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, GitBranch, PackageSearch, Plus, RefreshCw, Rocket, Server } from "lucide-react";
import { deploymentApi, getErrorMessage, secureCredentialApi, sshServerApi } from "@/lib/api";
import type {
  DeploymentAiAdviceResult,
  DeploymentCandidate,
  DeploymentDetectionResult,
  DeploymentEnvironmentProfile,
  DeploymentGroup,
  DeploymentImageStoreApp,
  DeploymentPlan,
  DeploymentPlanStage,
  DeploymentProbeCheck,
  DeploymentRun,
  DeploymentRunDetail,
  DeploymentRunStep,
  DeploymentTarget,
  DeploymentTemplate,
  DetectDeploymentProjectInput,
  InstallImageStoreAppInput,
  SecureCredential,
  SshServer,
  UpsertDeploymentGroupInput,
  UpsertDeploymentTargetInput,
} from "@/types";

const { Paragraph, Text, Title } = Typography;

const recipeMeta: Record<string, { label: string; color: string }> = {
  "image-store": { label: "镜像商店", color: "magenta" },
  "1panel-app": { label: "1Panel", color: "geekblue" },
  "dockerfile-service": { label: "Dockerfile", color: "blue" },
  "docker-compose": { label: "Compose", color: "cyan" },
  "static-openresty": { label: "静态站", color: "green" },
  "static-nginx": { label: "Nginx 静态站", color: "lime" },
  "node-pm2": { label: "Node PM2", color: "purple" },
  "systemd-binary": { label: "Systemd", color: "orange" },
  "custom-script": { label: "自定义脚本", color: "red" },
};

const riskMeta: Record<string, { label: string; color: string }> = {
  readonly: { label: "只读", color: "blue" },
  review: { label: "二次确认", color: "orange" },
  high: { label: "审批", color: "red" },
};

const gitCredentialProviders = new Set(["github", "gitlab", "gitcode", "gitee"]);

function isGitCredential(credential: SecureCredential) {
  if (gitCredentialProviders.has(credential.provider)) {
    return true;
  }
  const haystack = [
    credential.credentialKey,
    credential.displayName,
    credential.provider,
    credential.accountName,
    credential.baseUrl,
    credential.description,
    ...credential.scopes,
    ...credential.tags,
  ]
    .join(" ")
    .toLowerCase();
  return credential.provider === "custom" && /\bgit(hub|lab|code|ee)?\b|repo/.test(haystack);
}

function gitCredentialUnavailableReason(credential: SecureCredential) {
  if (!credential.enabled) {
    return "已禁用";
  }
  if (credential.status !== "active") {
    return "非 active";
  }
  if (!credential.hasSecret) {
    return "未保存密钥";
  }
  if (!credential.allowMcp) {
    return "未允许 MCP/部署使用";
  }
  if (credential.approvalPolicy === "blocked_for_mcp") {
    return "已阻止 MCP 使用";
  }
  return "";
}

function recipeTag(recipe: string) {
  const meta = recipeMeta[recipe] ?? { label: recipe, color: "default" };
  return <Tag color={meta.color}>{meta.label}</Tag>;
}

function riskTag(risk: string) {
  const meta = riskMeta[risk] ?? { label: risk, color: "default" };
  return <Tag color={meta.color}>{meta.label}</Tag>;
}

function probeStatusTag(status: string) {
  const color = status === "ok" ? "green" : status === "warning" ? "orange" : "default";
  const label = status === "ok" ? "正常" : status === "warning" ? "需确认" : status;
  return <Tag color={color}>{label}</Tag>;
}

function runStatusTag(status: string) {
  const meta: Record<string, { label: string; color: string }> = {
    pending: { label: "等待", color: "default" },
    running: { label: "执行中", color: "blue" },
    approval_required: { label: "待审批", color: "orange" },
    success: { label: "成功", color: "green" },
    failed: { label: "失败", color: "red" },
    blocked: { label: "已阻断", color: "red" },
  };
  const item = meta[status] ?? { label: status, color: "default" };
  return <Tag color={item.color}>{item.label}</Tag>;
}

function formatDisk(kb?: number | null) {
  if (!kb || kb <= 0) {
    return "-";
  }
  const gb = kb / 1024 / 1024;
  return `${gb.toFixed(gb >= 10 ? 0 : 1)} GB`;
}

function configSummary(target: DeploymentTarget) {
  const parts = [target.deployRoot, target.domain, target.port ? `:${target.port}` : ""].filter(Boolean);
  return parts.length ? parts.join(" ") : "-";
}

export default function DeploymentsPage() {
  const [targets, setTargets] = useState<DeploymentTarget[]>([]);
  const [groups, setGroups] = useState<DeploymentGroup[]>([]);
  const [runs, setRuns] = useState<DeploymentRun[]>([]);
  const [templates, setTemplates] = useState<DeploymentTemplate[]>([]);
  const [profiles, setProfiles] = useState<DeploymentEnvironmentProfile[]>([]);
  const [imageStoreApps, setImageStoreApps] = useState<DeploymentImageStoreApp[]>([]);
  const [selectedImageStoreApp, setSelectedImageStoreApp] = useState<DeploymentImageStoreApp | null>(null);
  const [imageStoreOpen, setImageStoreOpen] = useState(false);
  const [installingImageKey, setInstallingImageKey] = useState<string | null>(null);
  const [credentials, setCredentials] = useState<SecureCredential[]>([]);
  const [servers, setServers] = useState<SshServer[]>([]);
  const [loading, setLoading] = useState(false);
  const [targetDrawerOpen, setTargetDrawerOpen] = useState(false);
  const [groupDrawerOpen, setGroupDrawerOpen] = useState(false);
  const [detectOpen, setDetectOpen] = useState(false);
  const [detecting, setDetecting] = useState(false);
  const [detection, setDetection] = useState<DeploymentDetectionResult | null>(null);
  const [dryRunOpen, setDryRunOpen] = useState(false);
  const [dryRunLoading, setDryRunLoading] = useState(false);
  const [dryRunPlan, setDryRunPlan] = useState<DeploymentPlan | null>(null);
  const [aiAdviceLoading, setAiAdviceLoading] = useState(false);
  const [aiAdvice, setAiAdvice] = useState<DeploymentAiAdviceResult | null>(null);
  const [runDetailOpen, setRunDetailOpen] = useState(false);
  const [runDetailLoading, setRunDetailLoading] = useState(false);
  const [runDetail, setRunDetail] = useState<DeploymentRunDetail | null>(null);
  const [executingKey, setExecutingKey] = useState<string | null>(null);
  const [targetForm] = Form.useForm<UpsertDeploymentTargetInput>();
  const [groupForm] = Form.useForm<UpsertDeploymentGroupInput>();
  const [detectForm] = Form.useForm<DetectDeploymentProjectInput>();
  const [imageStoreForm] = Form.useForm<InstallImageStoreAppInput>();
  const targetSourceType = Form.useWatch("sourceType", targetForm);
  const detectSourceType = Form.useWatch("sourceType", detectForm);
  const profileGroups = useMemo(() => {
    const groups: Record<string, DeploymentEnvironmentProfile[]> = {};
    for (const profile of profiles) {
      const category = profile.category || "基础模板";
      groups[category] = [...(groups[category] || []), profile];
    }
    return Object.entries(groups);
  }, [profiles]);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [
        nextTargets,
        nextGroups,
        nextTemplates,
        nextProfiles,
        nextImageStoreApps,
        nextRuns,
        nextCredentials,
        nextServers,
      ] = await Promise.all([
        deploymentApi.listTargets(),
        deploymentApi.listGroups(),
        deploymentApi.listTemplates(),
        deploymentApi.listEnvironmentProfiles(),
        deploymentApi.listImageStoreApps(),
        deploymentApi.listRuns({ status: "all", limit: 50 }),
        secureCredentialApi.list(),
        sshServerApi.list(),
      ]);
      setTargets(nextTargets);
      setGroups(nextGroups);
      setTemplates(nextTemplates);
      setProfiles(nextProfiles);
      setImageStoreApps(nextImageStoreApps);
      setRuns(nextRuns);
      setCredentials(nextCredentials);
      setServers(nextServers);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const targetOptions = useMemo(
    () => targets.map((target) => ({ label: `${target.name} (${target.targetKey})`, value: target.targetKey })),
    [targets],
  );

  const recipeOptions = useMemo(
    () => templates.map((template) => ({ label: template.name, value: template.key })),
    [templates],
  );

  const serverOptions = useMemo(
    () =>
      servers.map((server) => ({
        label: `${server.alias} (${server.username}@${server.host}:${server.port})`,
        value: server.alias,
        disabled: !server.enabled,
      })),
    [servers],
  );

  const gitCredentialOptions = useMemo(
    () =>
      credentials
        .filter(isGitCredential)
        .map((credential) => {
          const reason = gitCredentialUnavailableReason(credential);
          return {
            label: reason
              ? `${credential.displayName} (${credential.credentialKey}) - ${reason}`
              : `${credential.displayName} (${credential.credentialKey})`,
            value: credential.credentialKey,
            disabled: Boolean(reason),
            title: `${credential.provider} / ${credential.accountName || "-"} / ${credential.scopes.join(", ") || "-"}`,
          };
        }),
    [credentials],
  );

  async function chooseTargetProjectDirectory() {
    try {
      const selected = await openDialog({ directory: true, multiple: false, title: "选择本地项目目录" });
      if (typeof selected === "string") {
        targetForm.setFieldValue("projectPath", selected);
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function chooseDetectProjectDirectory() {
    try {
      const selected = await openDialog({ directory: true, multiple: false, title: "选择本地项目目录" });
      if (typeof selected === "string") {
        detectForm.setFieldValue("projectPath", selected);
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  function openTargetDrawer(record?: DeploymentTarget) {
    targetForm.resetFields();
    if (record) {
      targetForm.setFieldsValue({
        id: record.id,
        targetKey: record.targetKey,
        name: record.name,
        serverAlias: record.serverAlias,
        recipe: record.recipe,
        sourceType: record.sourceType,
        projectPath: record.projectPath,
        gitUrl: record.gitUrl,
        gitRef: record.gitRef,
        gitCredentialKey: record.gitCredentialKey,
        dockerBuildMode: record.dockerBuildMode,
        workdir: record.workdir,
        deployRoot: record.deployRoot,
        domain: record.domain,
        httpsEnabled: record.httpsEnabled,
        port: record.port,
        healthCheckUrl: record.healthCheckUrl,
        configJson: record.configJson,
        enabled: record.enabled,
      });
    } else {
      targetForm.setFieldsValue({
        sourceType: "local",
        recipe: "dockerfile-service",
        dockerBuildMode: "remote",
        httpsEnabled: false,
        enabled: true,
        configJson: "{}",
      });
    }
    setTargetDrawerOpen(true);
  }

  function applyTemplate(template: DeploymentTemplate) {
    const keySeed = template.key.replace(/[^a-zA-Z0-9_-]/g, "-");
    const defaults: Partial<UpsertDeploymentTargetInput> = {
      targetKey: `my-${keySeed}`,
      name: template.name,
      serverAlias: "",
      recipe: template.key,
      sourceType: template.supportedSources.includes("local") ? "local" : template.supportedSources[0] || "local",
      projectPath: "",
      gitUrl: "",
      gitRef: "main",
      gitCredentialKey: "",
      dockerBuildMode: "remote",
      workdir: ".",
      deployRoot: `/opt/tauri-ssh/stacks/my-${keySeed}`,
      domain: "",
      httpsEnabled: false,
      healthCheckUrl: "",
      configJson: "{}",
      enabled: true,
    };
    if (template.key === "docker-compose") {
      defaults.workdir = ".";
    }
    if (template.key === "1panel-app") {
      defaults.recipe = "1panel-app";
      defaults.workdir = ".";
      defaults.port = undefined;
      defaults.configJson = JSON.stringify({ deploymentProfile: "1panel-app" }, null, 2);
    }
    if (template.key === "static-openresty" || template.key === "static-nginx") {
      defaults.port = 80;
      defaults.healthCheckUrl = "/";
    }
    if (template.key === "dockerfile-service") {
      defaults.port = 8080;
    }
    targetForm.resetFields();
    targetForm.setFieldsValue(defaults);
    setTargetDrawerOpen(true);
    message.success(`已应用模板：${template.name}`);
  }

  function applyEnvironmentProfile(profile: DeploymentEnvironmentProfile) {
    const keySeed = profile.key.replace(/[^a-zA-Z0-9_-]/g, "-");
    const disabledServiceAccounts = {
      serviceAccounts: {
        database: {
          enabled: false,
          connectionKey: "",
          databaseName: `my_${keySeed.replace(/-/g, "_")}`,
          username: `${keySeed.replace(/-/g, "_")}_app`,
          credentialKey: `${keySeed}_mysql_app`,
        },
        redis: {
          enabled: false,
          connectionKey: "",
          databaseName: "0",
          username: `${keySeed.replace(/-/g, "_")}_redis`,
          credentialKey: `${keySeed}_redis_app`,
        },
      },
    };
    const defaults: Partial<UpsertDeploymentTargetInput> = {
      targetKey: `my-${keySeed}`,
      name: profile.name,
      serverAlias: "",
      recipe: "dockerfile-service",
      sourceType: "local",
      projectPath: "",
      gitUrl: "",
      gitRef: "main",
      gitCredentialKey: "",
      dockerBuildMode: "remote",
      workdir: ".",
      deployRoot: `/opt/tauri-ssh/stacks/my-${keySeed}`,
      domain: "",
      httpsEnabled: false,
      port: 8080,
      healthCheckUrl: "",
      configJson: JSON.stringify({ deploymentProfile: profile.key }, null, 2),
      enabled: true,
    };
    if (profile.key === "1panel-app") {
      defaults.recipe = "1panel-app";
      defaults.port = undefined;
      defaults.configJson = JSON.stringify({ deploymentProfile: profile.key }, null, 2);
    }
    if (profile.key === "custom-script") {
      defaults.recipe = "custom-script";
      defaults.port = undefined;
      defaults.configJson = JSON.stringify(
        {
          deploymentProfile: profile.key,
          customStages: [
            {
              key: "deploy",
              title: "执行自定义部署脚本",
              command: "echo '请在扩展配置 JSON 中替换为真实部署命令'",
              risk: "high",
              approvalRequired: true,
              summary: "自定义脚本会进入危险命令扫描和审批流程。",
            },
          ],
        },
        null,
        2,
      );
    }
    if (profile.key === "docker-compose") {
      defaults.recipe = "docker-compose";
      defaults.port = undefined;
      defaults.configJson = JSON.stringify({ deploymentProfile: profile.key }, null, 2);
    }
    if (profile.key === "static-nginx") {
      defaults.recipe = "static-nginx";
      defaults.port = 80;
      defaults.healthCheckUrl = "/";
      defaults.configJson = JSON.stringify({ deploymentProfile: profile.key }, null, 2);
    }
    if (profile.key === "static-openresty") {
      defaults.recipe = "static-openresty";
      defaults.httpsEnabled = false;
      defaults.port = 80;
      defaults.healthCheckUrl = "/";
      defaults.configJson = JSON.stringify({ deploymentProfile: profile.key }, null, 2);
    }
    if (profile.key === "node-pm2") {
      defaults.recipe = "node-pm2";
      defaults.port = undefined;
      defaults.configJson = JSON.stringify({ deploymentProfile: profile.key }, null, 2);
    }
    if (profile.key === "systemd-binary") {
      defaults.recipe = "systemd-binary";
      defaults.port = undefined;
      defaults.configJson = JSON.stringify({ deploymentProfile: profile.key }, null, 2);
    }
    if (profile.key === "static-openresty-https") {
      defaults.recipe = "static-openresty";
      defaults.port = 80;
      defaults.healthCheckUrl = "/";
      defaults.configJson = JSON.stringify(
        {
          deploymentProfile: profile.key,
          web: {
            runtime: "openresty",
            documentRoot: "dist",
            https: {
              enabled: false,
              domain: "",
              note: "填写域名并勾选 HTTPS 后，dry-run 会生成自动签证书阶段。",
            },
          },
        },
        null,
        2,
      );
    }
    if (profile.key === "springboot-mysql-redis") {
      defaults.recipe = "systemd-binary";
      defaults.port = 8080;
      defaults.healthCheckUrl = "/";
      defaults.configJson = JSON.stringify(
        {
          deploymentProfile: profile.key,
          runtime: {
            kind: "spring-boot",
            serviceName: `my-${keySeed}`,
            startCommand: "java -jar app.jar",
          },
          ...disabledServiceAccounts,
        },
        null,
        2,
      );
    }
    if (profile.key === "compose-db-redis") {
      defaults.recipe = "docker-compose";
      defaults.port = undefined;
      defaults.configJson = JSON.stringify(
        {
          deploymentProfile: profile.key,
          compose: {
            mode: "reuse-shared-middleware",
            networkMode: "host",
          },
          ...disabledServiceAccounts,
        },
        null,
        2,
      );
    }
    if (profile.key === "frontend-api-same-domain") {
      defaults.recipe = "static-openresty";
      defaults.port = 80;
      defaults.healthCheckUrl = "/";
      defaults.configJson = JSON.stringify(
        {
          deploymentProfile: profile.key,
          web: {
            runtime: "openresty",
            documentRoot: "dist",
            apiProxy: {
              prefix: "/api/",
              upstreamHost: "127.0.0.1",
              upstreamPort: 8080,
              websocket: true,
            },
            https: {
              enabled: false,
              domain: "",
            },
          },
        },
        null,
        2,
      );
    }
    if (profile.key === "1panel-app-db") {
      defaults.recipe = "1panel-app";
      defaults.port = undefined;
      defaults.configJson = JSON.stringify(
        {
          deploymentProfile: profile.key,
          panel: {
            provider: "1panel",
            appDir: "",
            composeService: "",
          },
          ...disabledServiceAccounts,
        },
        null,
        2,
      );
    }
    targetForm.resetFields();
    targetForm.setFieldsValue(defaults);
    setTargetDrawerOpen(true);
    message.success(`已应用环境方案：${profile.name}`);
  }

  function openImageStoreInstall(app: DeploymentImageStoreApp) {
    const targetKey = `img-${app.key}`;
    const envDefaults = Object.fromEntries(app.env.map((item) => [item.key, item.defaultValue]));
    setSelectedImageStoreApp(app);
    imageStoreForm.resetFields();
    imageStoreForm.setFieldsValue({
      appKey: app.key,
      targetKey,
      name: app.name,
      serverAlias: "",
      port: app.defaultPort,
      deployRoot: `/opt/tauri-ssh/stacks/${targetKey}`,
      imageTag: app.tag,
      envJson: JSON.stringify(envDefaults, null, 2),
      enabled: true,
    });
    setImageStoreOpen(true);
  }

  async function installImageStoreApp() {
    if (!selectedImageStoreApp) return;
    setInstallingImageKey(selectedImageStoreApp.key);
    try {
      const values = await imageStoreForm.validateFields();
      const target = await deploymentApi.installImageStoreApp(values);
      setImageStoreOpen(false);
      message.success(`已生成镜像部署目标：${target.name}`);
      await loadData();
      await runDryRun({ targetKey: target.targetKey });
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setInstallingImageKey(null);
    }
  }

  function openGroupDrawer(record?: DeploymentGroup) {
    groupForm.resetFields();
    if (record) {
      groupForm.setFieldsValue({
        id: record.id,
        groupKey: record.groupKey,
        name: record.name,
        description: record.description,
        enabled: record.enabled,
        targets: record.targets.map((item) => ({
          targetKey: item.targetKey,
          sortOrder: item.sortOrder,
          enabled: item.enabled,
        })),
      });
    } else {
      groupForm.setFieldsValue({ enabled: true, targets: [] });
    }
    setGroupDrawerOpen(true);
  }

  async function saveTarget() {
    try {
      const values = await targetForm.validateFields();
      await deploymentApi.upsertTarget(values);
      message.success("部署目标已保存");
      setTargetDrawerOpen(false);
      await loadData();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function saveGroup() {
    try {
      const values = await groupForm.validateFields();
      await deploymentApi.upsertGroup(values);
      message.success("部署组已保存");
      setGroupDrawerOpen(false);
      await loadData();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function deleteTarget(targetKey: string) {
    try {
      await deploymentApi.deleteTarget(targetKey);
      message.success("部署目标已删除");
      await loadData();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function deleteGroup(groupKey: string) {
    try {
      await deploymentApi.deleteGroup(groupKey);
      message.success("部署组已删除");
      await loadData();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function runDetection() {
    setDetecting(true);
    try {
      const values = await detectForm.validateFields();
      const result = await deploymentApi.detectProject(values);
      setDetection(result);
      if (result.candidates.length === 0) {
        message.warning("未识别到可部署目标");
      } else {
        message.success(`识别到 ${result.candidates.length} 个候选目标`);
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setDetecting(false);
    }
  }

  async function runDryRun(input: { targetKey?: string; groupKey?: string }) {
    setDryRunLoading(true);
    setDryRunOpen(true);
    setDryRunPlan(null);
    setAiAdvice(null);
    try {
      const plan = await deploymentApi.createDryRun(input);
      setDryRunPlan(plan);
      message.success("Dry-run 计划已生成");
    } catch (error) {
      message.error(getErrorMessage(error));
      setDryRunOpen(false);
    } finally {
      setDryRunLoading(false);
    }
  }

  async function runRollbackDryRun(targetKey: string) {
    setDryRunLoading(true);
    setDryRunOpen(true);
    setDryRunPlan(null);
    setAiAdvice(null);
    try {
      const plan = await deploymentApi.createRollbackDryRun({ targetKey });
      setDryRunPlan(plan);
      message.success("回滚 Dry-run 计划已生成");
    } catch (error) {
      message.error(getErrorMessage(error));
      setDryRunOpen(false);
    } finally {
      setDryRunLoading(false);
    }
  }

  async function executeRun(input: { targetKey?: string; groupKey?: string; continueRunId?: string }, key: string) {
    setExecutingKey(key);
    try {
      const detail = await deploymentApi.executeRun({ ...input, createdBy: "local-user" });
      setRunDetail(detail);
      setRunDetailOpen(true);
      await loadData();
      if (detail.run.status === "approval_required") {
        message.warning("已执行可自动步骤，后续步骤已进入审批队列");
      } else if (detail.run.status === "success") {
        message.success("部署执行完成");
      } else {
        message.error("部署执行失败，请查看步骤日志");
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setExecutingKey(null);
    }
  }

  async function executeRollback(targetKey: string) {
    const key = `rollback:${targetKey}`;
    setExecutingKey(key);
    try {
      const detail = await deploymentApi.executeRollback({ targetKey, createdBy: "local-user" });
      setRunDetail(detail);
      setRunDetailOpen(true);
      await loadData();
      if (detail.run.status === "approval_required") {
        message.warning("回滚步骤已进入审批队列");
      } else if (detail.run.status === "success") {
        message.success("回滚执行完成");
      } else {
        message.error("回滚执行失败，请查看步骤日志");
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setExecutingKey(null);
    }
  }

  async function askAiAdvice() {
    if (!dryRunPlan) {
      return;
    }
    setAiAdviceLoading(true);
    setAiAdvice(null);
    try {
      const result = await deploymentApi.askAiAdvice({
        plan: dryRunPlan,
        prompt: "请分析这个自动部署 dry-run 计划，给出部署建议、风险解释、审批关注点和执行前检查清单。",
      });
      setAiAdvice(result);
      message.success("AI 部署建议已生成");
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setAiAdviceLoading(false);
    }
  }

  async function openRunDetail(runId: string) {
    setRunDetailLoading(true);
    setRunDetailOpen(true);
    try {
      const detail = await deploymentApi.getRunDetail(runId);
      setRunDetail(detail);
    } catch (error) {
      message.error(getErrorMessage(error));
      setRunDetailOpen(false);
    } finally {
      setRunDetailLoading(false);
    }
  }

  function applyCandidate(candidate: DeploymentCandidate) {
    const sourceType = detection?.sourceType ?? candidate.sourceType;
    targetForm.resetFields();
    targetForm.setFieldsValue({
      targetKey: candidate.key,
      name: candidate.name,
      serverAlias: "",
      recipe: candidate.recipe,
      sourceType,
      projectPath: sourceType === "local" ? detection?.projectRoot : "",
      gitUrl: detection?.gitUrl,
      gitRef: detection?.gitRef,
      gitCredentialKey: detectForm.getFieldValue("gitCredentialKey") ?? "",
      dockerBuildMode: "remote",
      workdir: candidate.workdir,
      deployRoot: `/opt/tauri-ssh/stacks/${candidate.key}`,
      httpsEnabled: false,
      port: candidate.exposedPorts[0],
      healthCheckUrl: "",
      configJson: candidate.configJson || "{}",
      enabled: true,
    });
    setDetectOpen(false);
    setTargetDrawerOpen(true);
  }

  const targetColumns: ColumnsType<DeploymentTarget> = [
    {
      title: "目标",
      dataIndex: "name",
      width: 220,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Text strong>{record.name}</Text>
          <Text type="secondary">{record.targetKey}</Text>
        </Space>
      ),
    },
    {
      title: "配方",
      dataIndex: "recipe",
      width: 130,
      render: (value) => recipeTag(String(value)),
    },
    {
      title: "来源",
      width: 140,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Tag color={record.sourceType === "git" ? "purple" : "blue"}>
            {record.sourceType === "git" ? "Git 仓库" : "本地目录"}
          </Tag>
          <Text type="secondary">{record.sourceType === "git" ? record.gitRef || "默认分支" : record.workdir || "."}</Text>
        </Space>
      ),
    },
    { title: "服务器", dataIndex: "serverAlias", width: 140 },
    {
      title: "部署配置",
      width: 260,
      render: (_, record) => configSummary(record),
    },
    {
      title: "状态",
      dataIndex: "enabled",
      width: 90,
      render: (value) => <Tag color={value ? "green" : "default"}>{value ? "启用" : "禁用"}</Tag>,
    },
    {
      title: "操作",
      width: 420,
      render: (_, record) => (
        <Space>
          <Button size="small" type="primary" ghost onClick={() => void runDryRun({ targetKey: record.targetKey })}>
            Dry-run
          </Button>
          <Button
            size="small"
            type="primary"
            loading={executingKey === `target:${record.targetKey}`}
            onClick={() => void executeRun({ targetKey: record.targetKey }, `target:${record.targetKey}`)}
          >
            执行
          </Button>
          <Button size="small" onClick={() => void runRollbackDryRun(record.targetKey)}>
            回滚预览
          </Button>
          <Button
            size="small"
            danger
            loading={executingKey === `rollback:${record.targetKey}`}
            onClick={() => void executeRollback(record.targetKey)}
          >
            回滚
          </Button>
          <Button size="small" onClick={() => openTargetDrawer(record)}>
            编辑
          </Button>
          <Popconfirm title="确认删除该部署目标？" onConfirm={() => void deleteTarget(record.targetKey)}>
            <Button size="small" danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const groupColumns: ColumnsType<DeploymentGroup> = [
    {
      title: "部署组",
      dataIndex: "name",
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Text strong>{record.name}</Text>
          <Text type="secondary">{record.groupKey}</Text>
        </Space>
      ),
    },
    { title: "描述", dataIndex: "description" },
    {
      title: "目标",
      dataIndex: "targets",
      render: (value: DeploymentGroup["targets"]) => (
        <Space wrap>
          {value.map((target) => (
            <Tag key={target.targetKey}>{target.targetName}</Tag>
          ))}
        </Space>
      ),
    },
    {
      title: "状态",
      dataIndex: "enabled",
      width: 90,
      render: (value) => <Tag color={value ? "green" : "default"}>{value ? "启用" : "禁用"}</Tag>,
    },
    {
      title: "操作",
      width: 420,
      render: (_, record) => (
        <Space>
          <Button size="small" type="primary" ghost onClick={() => void runDryRun({ groupKey: record.groupKey })}>
            Dry-run
          </Button>
          <Button
            size="small"
            type="primary"
            loading={executingKey === `group:${record.groupKey}`}
            onClick={() => void executeRun({ groupKey: record.groupKey }, `group:${record.groupKey}`)}
          >
            执行
          </Button>
          <Button size="small" onClick={() => openGroupDrawer(record)}>
            编辑
          </Button>
          <Popconfirm title="确认删除该部署组？" onConfirm={() => void deleteGroup(record.groupKey)}>
            <Button size="small" danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const candidateColumns: ColumnsType<DeploymentCandidate> = [
    {
      title: "候选目标",
      dataIndex: "name",
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Text strong>{record.name}</Text>
          <Text type="secondary">{record.workdir}</Text>
        </Space>
      ),
    },
    { title: "配方", dataIndex: "recipe", render: (value) => recipeTag(String(value)) },
    { title: "置信度", dataIndex: "confidence", render: (value) => `${value}%` },
    {
      title: "框架",
      dataIndex: "detectedFrameworks",
      render: (value: string[]) => value.map((item) => <Tag key={item}>{item}</Tag>),
    },
    {
      title: "操作",
      width: 110,
      render: (_, record) => (
        <Button type="primary" size="small" onClick={() => applyCandidate(record)}>
          创建目标
        </Button>
      ),
    },
  ];

  const probeColumns: ColumnsType<DeploymentProbeCheck> = [
    { title: "检查项", dataIndex: "label", width: 150 },
    { title: "状态", dataIndex: "status", width: 100, render: (value) => probeStatusTag(String(value)) },
    { title: "说明", dataIndex: "message" },
  ];

  const stageColumns: ColumnsType<DeploymentPlanStage> = [
    {
      title: "阶段",
      dataIndex: "title",
      width: 150,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Text strong>{record.title}</Text>
          <Text type="secondary">{record.key}</Text>
        </Space>
      ),
    },
    { title: "风险", dataIndex: "risk", width: 110, render: (value) => riskTag(String(value)) },
    {
      title: "审批",
      dataIndex: "approvalRequired",
      width: 100,
      render: (value) => <Tag color={value ? "red" : "green"}>{value ? "需要" : "不需要"}</Tag>,
    },
    {
      title: "命令预览",
      dataIndex: "commandPreview",
      width: 300,
      render: (value) =>
        value ? (
          <Text code copyable>
            {value}
          </Text>
        ) : (
          <Text type="secondary">-</Text>
        ),
    },
    { title: "说明", dataIndex: "summary" },
  ];

  const runColumns: ColumnsType<DeploymentRun> = [
    {
      title: "运行",
      dataIndex: "runId",
      width: 210,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Text strong>{record.runId}</Text>
          <Text type="secondary">{record.createdAt}</Text>
        </Space>
      ),
    },
    { title: "目标", dataIndex: "targetKey", width: 160 },
    { title: "状态", dataIndex: "status", width: 110, render: (value) => runStatusTag(String(value)) },
    { title: "摘要", dataIndex: "summary" },
    {
      title: "操作",
      width: 190,
      render: (_, record) => (
        <Space>
          <Button size="small" onClick={() => void openRunDetail(record.runId)}>
            详情
          </Button>
          {record.status === "approval_required" && (
            <Button
              size="small"
              type="primary"
              loading={executingKey === `run:${record.runId}`}
              onClick={() => void executeRun({ continueRunId: record.runId }, `run:${record.runId}`)}
            >
              继续
            </Button>
          )}
        </Space>
      ),
    },
  ];

  const runStepColumns: ColumnsType<DeploymentRunStep> = [
    {
      title: "步骤",
      dataIndex: "title",
      width: 160,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Text strong>{record.title}</Text>
          <Text type="secondary">{record.stepKey}</Text>
        </Space>
      ),
    },
    { title: "状态", dataIndex: "status", width: 110, render: (value) => runStatusTag(String(value)) },
    {
      title: "审批",
      dataIndex: "approvalId",
      width: 100,
      render: (value) => (value ? <Tag color="orange">#{value}</Tag> : <Text type="secondary">-</Text>),
    },
    {
      title: "命令",
      dataIndex: "commandPreview",
      width: 320,
      render: (value) =>
        value ? (
          <Text code copyable>
            {value}
          </Text>
        ) : (
          <Text type="secondary">-</Text>
        ),
    },
    {
      title: "输出",
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Text type={record.exitCode && record.exitCode !== 0 ? "danger" : undefined}>
            exit: {record.exitCode ?? "-"}
          </Text>
          <Text type="secondary">{record.stdoutPreview || record.stderrPreview || "-"}</Text>
        </Space>
      ),
    },
  ];

  const tabItems = [
    {
      key: "image-store",
      label: "镜像商店",
      children: (
        <div className="prototype-grid prototype-grid-3">
          {imageStoreApps.map((app) => (
            <Card
              key={app.key}
              title={app.name}
              extra={<Tag color="magenta">{app.category}</Tag>}
              actions={[
                <Button
                  key="install"
                  type="link"
                  loading={installingImageKey === app.key}
                  onClick={() => openImageStoreInstall(app)}
                >
                  一键安装
                </Button>,
              ]}
            >
              <Space direction="vertical" size="small">
                <Space wrap>
                  <Tag color="blue">{app.image}:{app.tag}</Tag>
                  {app.defaultPort ? <Tag>端口 {app.defaultPort}</Tag> : null}
                  {app.containerPort ? <Tag>容器 {app.containerPort}</Tag> : null}
                </Space>
                <Paragraph type="secondary">{app.description}</Paragraph>
                <Text type="secondary">数据目录：{app.volumePath}</Text>
                {app.notes.length > 0 && (
                  <Space direction="vertical" size={2}>
                    {app.notes.map((note) => (
                      <Text key={note} type="secondary">
                        {note}
                      </Text>
                    ))}
                  </Space>
                )}
              </Space>
            </Card>
          ))}
        </div>
      ),
    },
    {
      key: "targets",
      label: "部署目标",
      children: (
        <Card>
          <Table
            rowKey="targetKey"
            loading={loading}
            dataSource={targets}
            columns={targetColumns}
            pagination={{ pageSize: 10 }}
            scroll={{ x: 1460 }}
          />
        </Card>
      ),
    },
    {
      key: "groups",
      label: "部署组",
      children: (
        <Card>
          <Table
            rowKey="groupKey"
            loading={loading}
            dataSource={groups}
            columns={groupColumns}
            pagination={{ pageSize: 10 }}
          />
        </Card>
      ),
    },
    {
      key: "templates",
      label: "部署模板",
      children: (
        <div className="prototype-grid prototype-grid-3">
          {templates.map((template) => (
            <Card
              key={template.key}
              title={template.name}
              actions={[
                <Button key="apply" type="link" onClick={() => applyTemplate(template)}>
                  使用模板
                </Button>,
              ]}
            >
              <Space direction="vertical" size="small">
                <Space>
                  {recipeTag(template.key)}
                  {riskTag(template.risk)}
                </Space>
                <Paragraph type="secondary">{template.description}</Paragraph>
                <Text>场景：{template.scenario}</Text>
                <Text>环境：{template.requiredProfiles.join(" / ") || "-"}</Text>
              </Space>
            </Card>
          ))}
        </div>
      ),
    },
    {
      key: "runs",
      label: "运行记录",
      children: (
        <Card>
          <Table
            rowKey="runId"
            loading={loading}
            dataSource={runs}
            columns={runColumns}
            pagination={{ pageSize: 10 }}
          />
        </Card>
      ),
    },
    {
      key: "profiles",
      label: "环境方案",
      children: (
        <Space direction="vertical" size="large" className="w-full">
          {profileGroups.map(([category, items]) => (
            <div key={category}>
              <Title level={4} style={{ marginTop: 0, marginBottom: 12 }}>
                {category}
              </Title>
              <div className="prototype-grid prototype-grid-3">
                {items.map((profile) => (
                  <Card
                    key={profile.key}
                    title={profile.name}
                    extra={<Tag color={category === "组合方案" ? "blue" : "default"}>{category}</Tag>}
                    actions={[
                      <Button key="apply" type="link" onClick={() => applyEnvironmentProfile(profile)}>
                        使用方案
                      </Button>,
                    ]}
                  >
                    <Space direction="vertical" size="small">
                      <Paragraph type="secondary">{profile.description}</Paragraph>
                      <Space wrap>
                        {profile.checks.map((check) => (
                          <Tag key={check}>{check}</Tag>
                        ))}
                      </Space>
                    </Space>
                  </Card>
                ))}
              </div>
            </div>
          ))}
        </Space>
      ),
    },
  ];

  return (
    <div className="prototype-page">
      <div className="prototype-page-header">
        <div>
          <Title level={2} style={{ fontSize: 24, marginBottom: 8 }}>
            自动部署
          </Title>
          <Paragraph type="secondary">
            从本地目录或 Git 仓库识别可部署目标，先完成目标、部署组、模板和环境方案管理。
          </Paragraph>
        </div>
        <Space>
          <Button icon={<RefreshCw size={16} />} onClick={() => void loadData()}>
            刷新
          </Button>
          <Button icon={<PackageSearch size={16} />} onClick={() => setDetectOpen(true)}>
            检测项目
          </Button>
          <Button icon={<Plus size={16} />} onClick={() => openGroupDrawer()}>
            新建部署组
          </Button>
          <Button type="primary" icon={<Rocket size={16} />} onClick={() => openTargetDrawer()}>
            新建目标
          </Button>
        </Space>
      </div>

      <div className="prototype-grid prototype-grid-4">
        <Card>
          <Statistic title="部署目标" value={targets.length} prefix={<Rocket size={18} />} />
        </Card>
        <Card>
          <Statistic title="部署组" value={groups.length} prefix={<Server size={18} />} />
        </Card>
        <Card>
          <Statistic title="内置模板" value={templates.length} prefix={<PackageSearch size={18} />} />
        </Card>
        <Card>
          <Statistic title="环境方案" value={profiles.length} prefix={<GitBranch size={18} />} />
        </Card>
        <Card>
          <Statistic title="镜像商店" value={imageStoreApps.length} prefix={<PackageSearch size={18} />} />
        </Card>
      </div>

      <Alert
        className="mt-4"
        type="info"
        showIcon
        title="已启用部署执行、运行记录和回滚"
        description="支持 dry-run 环境探测、单目标/部署组顺序执行、运行步骤日志、高风险审批暂停、审批后继续，以及基于 releases/current 目录结构的回滚入口。"
      />

      <Tabs className="mt-4" items={tabItems} />

      <Modal
        title={selectedImageStoreApp ? `安装镜像：${selectedImageStoreApp.name}` : "安装镜像"}
        open={imageStoreOpen}
        onCancel={() => setImageStoreOpen(false)}
        onOk={() => void installImageStoreApp()}
        okText="生成部署计划"
        confirmLoading={Boolean(selectedImageStoreApp && installingImageKey === selectedImageStoreApp.key)}
        destroyOnHidden
      >
        {selectedImageStoreApp && (
          <Space direction="vertical" className="w-full" size="middle">
            <Alert
              type="info"
              showIcon
              message={`${selectedImageStoreApp.image}:${selectedImageStoreApp.tag}`}
              description="安装会先创建镜像商店部署目标，然后自动生成 dry-run。真实启动容器仍需要走自动部署审批流程。"
            />
            <Form form={imageStoreForm} layout="vertical">
              <Form.Item name="appKey" hidden>
                <Input />
              </Form.Item>
              <Form.Item name="targetKey" label="目标 Key" rules={[{ required: true, message: "请输入目标 Key" }]}>
                <Input autoCapitalize="off" />
              </Form.Item>
              <Form.Item name="name" label="显示名称" rules={[{ required: true, message: "请输入显示名称" }]}>
                <Input autoCapitalize="off" />
              </Form.Item>
              <Form.Item name="serverAlias" label="目标服务器" rules={[{ required: true, message: "请选择目标服务器" }]}>
                <Select
                  showSearch
                  placeholder="选择资产服务器"
                  options={serverOptions}
                  optionFilterProp="label"
                  notFoundContent="暂无服务器，请先到资产 -> 服务器添加"
                />
              </Form.Item>
              <Space.Compact block>
                <Form.Item name="imageTag" label="镜像 Tag" className="w-1/2">
                  <Input autoCapitalize="off" />
                </Form.Item>
                <Form.Item name="port" label="宿主端口" className="w-1/2">
                  <InputNumber className="w-full" min={1} max={65535} />
                </Form.Item>
              </Space.Compact>
              <Form.Item name="deployRoot" label="远程部署根目录">
                <Input autoCapitalize="off" />
              </Form.Item>
              <Form.Item name="envJson" label="环境变量 JSON">
                <Input.TextArea rows={5} autoCapitalize="off" />
              </Form.Item>
              <Form.Item name="enabled" label="启用" valuePropName="checked">
                <Switch />
              </Form.Item>
            </Form>
          </Space>
        )}
      </Modal>

      <Drawer
        title="部署目标"
        size="large"
        open={targetDrawerOpen}
        onClose={() => setTargetDrawerOpen(false)}
        extra={
          <Button type="primary" onClick={() => void saveTarget()}>
            保存
          </Button>
        }
        destroyOnHidden
      >
        <Form form={targetForm} layout="vertical">
          <Form.Item name="id" hidden>
            <Input />
          </Form.Item>
          <Form.Item name="targetKey" label="目标 Key" rules={[{ required: true, message: "请输入目标 Key" }]}>
            <Input placeholder="my-app-api" autoCapitalize="off" />
          </Form.Item>
          <Form.Item name="name" label="显示名称" rules={[{ required: true, message: "请输入显示名称" }]}>
            <Input placeholder="业务后端 API" autoCapitalize="off" />
          </Form.Item>
          <Form.Item name="serverAlias" label="目标服务器" rules={[{ required: true, message: "请选择目标服务器" }]}>
            <Select
              showSearch
              placeholder="选择资产服务器"
              options={serverOptions}
              optionFilterProp="label"
              notFoundContent="暂无服务器，请先到资产 -> 服务器添加"
            />
          </Form.Item>
          <Space.Compact block>
            <Form.Item name="sourceType" label="项目来源" className="w-1/2" rules={[{ required: true }]}>
              <Select
                onChange={(value) => {
                  if (value === "local") {
                    targetForm.setFieldsValue({ gitUrl: "", gitRef: "", gitCredentialKey: "" });
                  } else if (value === "image-store") {
                    targetForm.setFieldsValue({ projectPath: "", gitUrl: "", gitRef: "", gitCredentialKey: "" });
                  } else {
                    targetForm.setFieldValue("projectPath", "");
                  }
                }}
                options={[
                  { label: "本地目录", value: "local" },
                  { label: "Git 仓库", value: "git" },
                  { label: "镜像商店", value: "image-store" },
                ]}
              />
            </Form.Item>
            <Form.Item name="recipe" label="部署配方" className="w-1/2" rules={[{ required: true }]}>
              <Select options={recipeOptions} />
            </Form.Item>
          </Space.Compact>
          <Form.Item label="本地项目目录">
            <Space.Compact block>
              <Form.Item name="projectPath" noStyle>
                <Input
	                  readOnly
	                  disabled={targetSourceType === "git" || targetSourceType === "image-store"}
                  placeholder="/Users/bin/Documents/GitHub/example"
                  autoCapitalize="off"
                />
              </Form.Item>
              <Button
                icon={<FolderOpen size={16} />}
	                disabled={targetSourceType === "git" || targetSourceType === "image-store"}
                onClick={() => void chooseTargetProjectDirectory()}
              >
                选择
              </Button>
            </Space.Compact>
          </Form.Item>
          <Form.Item name="gitUrl" label="Git 仓库 URL">
            <Input
              disabled={targetSourceType === "image-store"}
              placeholder="https://github.com/org/repo.git"
              autoCapitalize="off"
            />
          </Form.Item>
          <Space.Compact block>
            <Form.Item name="gitRef" label="分支/Tag/Commit" className="w-1/2">
              <Input disabled={targetSourceType === "image-store"} placeholder="main / v1.0.0" autoCapitalize="off" />
            </Form.Item>
            <Form.Item name="gitCredentialKey" label="Git 凭证" className="w-1/2">
              <Select
                allowClear
                showSearch
	                disabled={targetSourceType === "image-store"}
	                placeholder={targetSourceType === "git" ? "从凭证库选择 Git 凭证" : "Git 来源时使用，可预先选择"}
                options={gitCredentialOptions}
                optionFilterProp="label"
                notFoundContent="暂无可用 Git 凭证"
              />
            </Form.Item>
          </Space.Compact>
          <Space.Compact block>
            <Form.Item name="dockerBuildMode" label="Dockerfile 构建方式" className="w-1/2">
              <Select
                options={[
                  { label: "远程构建（默认）", value: "remote" },
                  { label: "本地构建镜像后上传", value: "local_upload" },
                ]}
              />
            </Form.Item>
            <Form.Item name="port" label="服务端口" className="w-1/2">
              <InputNumber className="w-full" min={1} max={65535} />
            </Form.Item>
          </Space.Compact>
          <Form.Item name="workdir" label="项目工作目录">
            <Input placeholder="." autoCapitalize="off" />
          </Form.Item>
          <Form.Item name="deployRoot" label="远程部署根目录">
            <Input placeholder="/opt/tauri-ssh/stacks/my-app" autoCapitalize="off" />
          </Form.Item>
          <Space.Compact block>
            <Form.Item name="domain" label="域名" className="w-1/2">
              <Input placeholder="app.example.com" autoCapitalize="off" />
            </Form.Item>
            <Form.Item name="httpsEnabled" label="HTTPS 自动签证书" valuePropName="checked" className="w-1/2">
              <Switch />
            </Form.Item>
          </Space.Compact>
          <Form.Item name="healthCheckUrl" label="健康检查 URL">
            <Input placeholder="https://app.example.com/health" autoCapitalize="off" />
          </Form.Item>
          <Form.Item name="enabled" label="启用" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item name="configJson" label="扩展配置 JSON">
            <Input.TextArea rows={5} autoCapitalize="off" />
          </Form.Item>
        </Form>
      </Drawer>

      <Drawer
        title="部署组"
        size="large"
        open={groupDrawerOpen}
        onClose={() => setGroupDrawerOpen(false)}
        extra={
          <Button type="primary" onClick={() => void saveGroup()}>
            保存
          </Button>
        }
        destroyOnHidden
      >
        <Form form={groupForm} layout="vertical">
          <Form.Item name="id" hidden>
            <Input />
          </Form.Item>
          <Form.Item name="groupKey" label="部署组 Key" rules={[{ required: true, message: "请输入部署组 Key" }]}>
            <Input placeholder="my-app" autoCapitalize="off" />
          </Form.Item>
          <Form.Item name="name" label="部署组名称" rules={[{ required: true, message: "请输入部署组名称" }]}>
            <Input placeholder="业务系统" autoCapitalize="off" />
          </Form.Item>
          <Form.Item name="description" label="描述">
            <Input.TextArea rows={3} autoCapitalize="off" />
          </Form.Item>
          <Form.Item name="enabled" label="启用" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item label="目标列表">
            <Form.List name="targets">
              {(fields, { add, remove }) => (
                <Space direction="vertical" className="w-full">
                  {fields.map((field) => (
                    <Space key={field.key} align="start" className="w-full">
                      <Form.Item
                        {...field}
                        name={[field.name, "targetKey"]}
                        rules={[{ required: true, message: "请选择目标" }]}
                        className="min-w-[260px]"
                      >
                        <Select options={targetOptions} placeholder="选择部署目标" />
                      </Form.Item>
                      <Form.Item {...field} name={[field.name, "sortOrder"]} className="w-24">
                        <InputNumber min={0} placeholder="排序" />
                      </Form.Item>
                      <Form.Item {...field} name={[field.name, "enabled"]} valuePropName="checked">
                        <Switch />
                      </Form.Item>
                      <Button danger onClick={() => remove(field.name)}>
                        删除
                      </Button>
                    </Space>
                  ))}
                  <Button onClick={() => add({ enabled: true })}>添加目标</Button>
                </Space>
              )}
            </Form.List>
          </Form.Item>
        </Form>
      </Drawer>

      <Modal
        title="检测项目"
        width={960}
        open={detectOpen}
        mask={{ closable: true }}
        onCancel={() => setDetectOpen(false)}
        footer={[
          <Button key="cancel" onClick={() => setDetectOpen(false)}>
            关闭
          </Button>,
          <Button key="detect" type="primary" loading={detecting} onClick={() => void runDetection()}>
            开始检测
          </Button>,
        ]}
        destroyOnHidden
      >
        <Form
          form={detectForm}
          layout="vertical"
          initialValues={{ sourceType: "local", gitRef: "main" }}
        >
          <Form.Item name="sourceType" label="项目来源" rules={[{ required: true }]}>
            <Select
              onChange={(value) => {
                if (value === "local") {
                  detectForm.setFieldsValue({ gitUrl: "", gitCredentialKey: "" });
                } else {
                  detectForm.setFieldValue("projectPath", "");
                }
              }}
              options={[
                { label: "本地项目目录", value: "local" },
                { label: "Git 仓库", value: "git" },
              ]}
            />
          </Form.Item>
          <Form.Item label="本地项目目录">
            <Space.Compact block>
              <Form.Item name="projectPath" noStyle>
                <Input
                  readOnly
                  disabled={detectSourceType === "git"}
                  placeholder="/Users/bin/Documents/GitHub/example"
                  autoCapitalize="off"
                />
              </Form.Item>
              <Button
                icon={<FolderOpen size={16} />}
                disabled={detectSourceType === "git"}
                onClick={() => void chooseDetectProjectDirectory()}
              >
                选择
              </Button>
            </Space.Compact>
          </Form.Item>
          <Space.Compact block>
            <Form.Item name="gitUrl" label="Git 仓库 URL" className="w-1/2">
              <Input placeholder="https://github.com/org/repo.git" autoCapitalize="off" />
            </Form.Item>
            <Form.Item name="gitRef" label="分支/Tag/Commit" className="w-1/2">
              <Input placeholder="main" autoCapitalize="off" />
            </Form.Item>
          </Space.Compact>
          <Form.Item name="gitCredentialKey" label="Git 凭证">
            <Select
              allowClear
              showSearch
              placeholder={detectSourceType === "git" ? "从凭证库选择 Git 凭证" : "Git 来源时使用，可预先选择"}
              options={gitCredentialOptions}
              optionFilterProp="label"
              notFoundContent="暂无可用 Git 凭证"
            />
          </Form.Item>
        </Form>

        {detection && (
          <Space direction="vertical" className="w-full">
            <Descriptions size="small" bordered>
              <Descriptions.Item label="来源">{detection.sourceType}</Descriptions.Item>
              <Descriptions.Item label="项目目录">{detection.projectRoot}</Descriptions.Item>
              <Descriptions.Item label="Commit">{detection.commit || "-"}</Descriptions.Item>
            </Descriptions>
            <Table
              rowKey="key"
              dataSource={detection.candidates}
              columns={candidateColumns}
              pagination={{ pageSize: 6 }}
            />
          </Space>
        )}
      </Modal>

      <Drawer
        title="部署 Dry-run 计划"
        size="large"
        width={980}
        open={dryRunOpen}
        loading={dryRunLoading}
        onClose={() => setDryRunOpen(false)}
        destroyOnHidden
      >
        {dryRunPlan && (
          <Space direction="vertical" className="w-full" size="middle">
            <Alert
              type={dryRunPlan.approvalRequired ? "warning" : "info"}
              showIcon
              title={dryRunPlan.title}
              description="Dry-run 仅生成计划和风险预览，不会执行部署命令、签发证书或创建数据库/Redis 账号。"
              action={
                <Button size="small" loading={aiAdviceLoading} onClick={() => void askAiAdvice()}>
                  AI 分析风险
                </Button>
              }
            />
            {aiAdvice && (
              <Card
                size="small"
                title="AI 部署建议"
                extra={
                  <Text type="secondary">
                    {aiAdvice.providerName} / {aiAdvice.model} / {aiAdvice.latencyMs}ms
                  </Text>
                }
              >
                <pre className="m-0 max-h-80 overflow-auto whitespace-pre-wrap rounded bg-[var(--bg-secondary)] p-3 text-sm">
                  {aiAdvice.answer}
                </pre>
              </Card>
            )}
            <Descriptions size="small" bordered column={3}>
              <Descriptions.Item label="Plan ID">{dryRunPlan.planId}</Descriptions.Item>
              <Descriptions.Item label="目标">{dryRunPlan.targetKey}</Descriptions.Item>
              <Descriptions.Item label="服务器">{dryRunPlan.serverAlias}</Descriptions.Item>
              <Descriptions.Item label="配方">{recipeTag(dryRunPlan.recipe)}</Descriptions.Item>
              <Descriptions.Item label="风险">{riskTag(dryRunPlan.risk)}</Descriptions.Item>
              <Descriptions.Item label="审批">
                <Tag color={dryRunPlan.approvalRequired ? "red" : "green"}>
                  {dryRunPlan.approvalRequired ? "需要审批/二次确认" : "只读或低风险"}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="系统">{dryRunPlan.environment.os || "-"}</Descriptions.Item>
              <Descriptions.Item label="架构">{dryRunPlan.environment.arch || "-"}</Descriptions.Item>
              <Descriptions.Item label="可用磁盘">{formatDisk(dryRunPlan.environment.diskAvailableKb)}</Descriptions.Item>
            </Descriptions>

            {dryRunPlan.warnings.length > 0 && (
              <Alert
                type="warning"
                showIcon
                message="风险提示"
                description={
                  <ul className="m-0 pl-5">
                    {dryRunPlan.warnings.map((warning) => (
                      <li key={warning}>{warning}</li>
                    ))}
                  </ul>
                }
              />
            )}

            <Card title="环境探测">
              <Table
                rowKey="key"
                size="small"
                dataSource={dryRunPlan.environment.checks}
                columns={probeColumns}
                pagination={false}
              />
            </Card>

            <Card title="阶段计划与命令预览">
              <Table
                rowKey="key"
                size="small"
                dataSource={dryRunPlan.stages}
                columns={stageColumns}
                pagination={false}
                scroll={{ x: 980 }}
              />
            </Card>

            <Collapse
              items={[
                {
                  key: "raw",
                  label: "环境探测原始输出",
                  children: (
                    <pre className="m-0 max-h-72 overflow-auto whitespace-pre-wrap rounded bg-[var(--bg-secondary)] p-3 text-xs">
                      {dryRunPlan.environment.rawOutput || "-"}
                    </pre>
                  ),
                },
              ]}
            />
          </Space>
        )}
      </Drawer>

      <Drawer
        title="部署运行详情"
        size="large"
        width={1040}
        open={runDetailOpen}
        loading={runDetailLoading}
        onClose={() => setRunDetailOpen(false)}
        destroyOnHidden
      >
        {runDetail && (
          <Space direction="vertical" className="w-full" size="middle">
            <Descriptions size="small" bordered column={3}>
              <Descriptions.Item label="Run ID">{runDetail.run.runId}</Descriptions.Item>
              <Descriptions.Item label="目标">{runDetail.run.targetKey}</Descriptions.Item>
              <Descriptions.Item label="状态">{runStatusTag(runDetail.run.status)}</Descriptions.Item>
              <Descriptions.Item label="创建人">{runDetail.run.createdBy}</Descriptions.Item>
              <Descriptions.Item label="开始时间">{runDetail.run.startedAt || "-"}</Descriptions.Item>
              <Descriptions.Item label="结束时间">{runDetail.run.finishedAt || "-"}</Descriptions.Item>
              <Descriptions.Item label="摘要" span={3}>
                {runDetail.run.summary}
              </Descriptions.Item>
            </Descriptions>
            <Alert
              type={runDetail.run.status === "approval_required" ? "warning" : "info"}
              showIcon
              message={
                runDetail.run.status === "approval_required"
                  ? "部分高风险步骤已进入审批队列，审批通过后可点击运行记录中的“继续”。"
                  : "运行步骤日志已记录，可用于排查部署结果。"
              }
            />
            <Table
              rowKey="id"
              size="small"
              dataSource={runDetail.steps}
              columns={runStepColumns}
              pagination={false}
              scroll={{ x: 980 }}
            />
          </Space>
        )}
      </Drawer>
    </div>
  );
}
