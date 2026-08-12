import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Alert,
  Button,
  Card,
  Collapse,
  Form,
  Input,
  Result,
  Select,
  Space,
  Steps,
  Tag,
  Typography,
  message,
} from "antd";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  FolderPlus,
  GitBranch,
  RefreshCw,
} from "lucide-react";
import { getErrorMessage, gitWorkspaceApi } from "@/lib/api";
import {
  knowledgeAnalysisApi,
  knowledgeCatalogApi,
  knowledgeIngestionApi,
} from "@/lib/api/knowledge-domain";
import type { GitWorkspace } from "@/types/gitWorkspace";
import type {
  KnowledgeProject,
  KnowledgeSource,
  UpsertKnowledgeProjectInput,
} from "@/types";
import type { KnowledgeRepositoryBinding } from "@/types/knowledge-domain/catalog";
import { createProjectKey, workspaceDefaultBranch } from "./utils";

const { Paragraph, Text, Title } = Typography;

interface ProjectBasicsValues {
  name: string;
  description?: string;
}

interface RepositoryOption {
  workspaceKey: string;
  alias: string;
  role: string;
}

const stepTitles = ["项目名称", "选择仓库", "选择版本", "确认同步"];
const projectKeyAttemptStorageKey = "knowledge.project-setup.attempt";

interface ProjectKeyAttempt {
  name: string;
  projectKey: string;
}

function loadProjectKeyAttempt(): ProjectKeyAttempt | null {
  try {
    const value = window.sessionStorage.getItem(projectKeyAttemptStorageKey);
    if (!value) return null;
    const parsed: unknown = JSON.parse(value);
    if (typeof parsed === "object" && parsed != null) {
      const attempt = parsed as Record<string, unknown>;
      if (
        typeof attempt.name === "string" &&
        typeof attempt.projectKey === "string"
      ) {
        return { name: attempt.name, projectKey: attempt.projectKey };
      }
    }
  } catch {
    // 存储不可用时仍保留当前页面内的重试稳定性。
  }
  return null;
}

function saveProjectKeyAttempt(attempt: ProjectKeyAttempt) {
  try {
    window.sessionStorage.setItem(
      projectKeyAttemptStorageKey,
      JSON.stringify(attempt),
    );
  } catch {
    // 隐私模式或受限 WebView 不影响当前页面内的创建流程。
  }
}

function clearProjectKeyAttempt() {
  try {
    window.sessionStorage.removeItem(projectKeyAttemptStorageKey);
  } catch {
    // 存储不可用时无需额外处理。
  }
}

/**
 * 线性首次使用流程：用户只确认必要的业务信息，内部项目键和默认分支由系统生成。
 */
export default function ProjectSetupPage() {
  const navigate = useNavigate();
  const { projectId } = useParams();
  const existingProjectId = projectId ? Number(projectId) : null;
  const [step, setStep] = useState(0);
  const [workspaces, setWorkspaces] = useState<GitWorkspace[]>([]);
  const [workspacesLoading, setWorkspacesLoading] = useState(false);
  const [workspacesError, setWorkspacesError] = useState<string | null>(null);
  const [selectedWorkspaceKeys, setSelectedWorkspaceKeys] = useState<string[]>(
    [],
  );
  const [repositoryOptions, setRepositoryOptions] = useState<
    RepositoryOption[]
  >([]);
  const [creating, setCreating] = useState(false);
  const [projectBasics, setProjectBasics] = useState<ProjectBasicsValues>({
    name: "",
    description: "",
  });
  const [createdProject, setCreatedProject] = useState<KnowledgeProject | null>(
    null,
  );
  const [initialVersionName, setInitialVersionName] = useState("初始版本");
  const [setupError, setSetupError] = useState<string | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [syncSources, setSyncSources] = useState<KnowledgeSource[]>([]);
  const [releaseId, setReleaseId] = useState<number | null>(null);
  const [existingProject, setExistingProject] =
    useState<KnowledgeProject | null>(null);
  const [existingProjectLoading, setExistingProjectLoading] = useState(
    existingProjectId != null,
  );
  const [existingProjectError, setExistingProjectError] = useState<
    string | null
  >(null);
  const [restoredBindingWorkspaceKeys, setRestoredBindingWorkspaceKeys] =
    useState<string[]>([]);
  const [restoredBindings, setRestoredBindings] = useState<
    KnowledgeRepositoryBinding[]
  >([]);
  const projectKeyAttemptRef = useRef<ProjectKeyAttempt | null>(
    loadProjectKeyAttempt(),
  );
  const [form] = Form.useForm<ProjectBasicsValues>();

  const selectedWorkspaces = useMemo(
    () =>
      workspaces.filter((workspace) =>
        selectedWorkspaceKeys.includes(workspace.workspaceKey),
      ),
    [selectedWorkspaceKeys, workspaces],
  );
  const unavailableRestoredWorkspaceKeys = useMemo(
    () =>
      restoredBindingWorkspaceKeys.filter(
        (workspaceKey) =>
          !workspaces.some(
            (workspace) => workspace.workspaceKey === workspaceKey,
          ),
      ),
    [restoredBindingWorkspaceKeys, workspaces],
  );
  const reusesSavedBindings =
    existingProject != null && restoredBindings.length > 0;

  function versionBranchFor(workspaceKey: string, fallback: string) {
    return (
      restoredBindings.find((binding) => binding.workspaceKey === workspaceKey)
        ?.defaultBranch || fallback
    );
  }

  const loadWorkspaces = useCallback(async () => {
    setWorkspacesLoading(true);
    setWorkspacesError(null);
    try {
      setWorkspaces(await gitWorkspaceApi.list({}));
    } catch (error) {
      setWorkspacesError(getErrorMessage(error));
    } finally {
      setWorkspacesLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadWorkspaces();
  }, [loadWorkspaces]);

  useEffect(() => {
    if (existingProjectId == null) return;
    if (!Number.isSafeInteger(existingProjectId) || existingProjectId < 1) {
      setExistingProjectError("项目地址无效");
      setExistingProjectLoading(false);
      return;
    }
    const verifiedProjectId = existingProjectId;
    let disposed = false;
    async function loadExistingProject() {
      setExistingProjectLoading(true);
      setExistingProjectError(null);
      try {
        const result = await knowledgeCatalogApi.listProjects({
          projectId: verifiedProjectId,
          limit: 100,
          offset: 0,
        });
        const project =
          result.items.find((item) => item.id === existingProjectId) ?? null;
        if (disposed) return;
        if (!project) {
          setExistingProjectError("没有找到这个项目");
          return;
        }
        const [bindings, releases] = await Promise.all([
          knowledgeCatalogApi.listRepositoryBindings(verifiedProjectId),
          knowledgeCatalogApi.listReleases(verifiedProjectId),
        ]);
        if (releases.length) {
          setExistingProjectError(
            "这个项目已经登记版本，可在项目版本页继续管理",
          );
          return;
        }
        setExistingProject(project);
        // 版本登记失败时，项目和仓库绑定可能已经成功保存。保留这些已确认的数据，
        // 让用户只完成剩余步骤，而不是重新选择并覆盖仓库设置。
        const workspaceKeys = bindings.map((binding) => binding.workspaceKey);
        setRestoredBindings(bindings);
        setRestoredBindingWorkspaceKeys(workspaceKeys);
        setSelectedWorkspaceKeys(workspaceKeys);
        setRepositoryOptions(
          bindings.map((binding) => ({
            workspaceKey: binding.workspaceKey,
            alias: binding.alias,
            role: binding.repositoryRole,
          })),
        );
        form.setFieldsValue({
          name: project.name,
          description: project.description,
        });
        setProjectBasics({
          name: project.name,
          description: project.description,
        });
        // 项目已经保存时跳过项目名称；保留仓库确认页，让用户在工作区暂时不可用时
        // 能先看到明确原因，而不是在最后一步才发现无法继续。
        setStep(1);
      } catch (error) {
        if (!disposed) setExistingProjectError(getErrorMessage(error));
      } finally {
        if (!disposed) setExistingProjectLoading(false);
      }
    }
    void loadExistingProject();
    return () => {
      disposed = true;
    };
  }, [existingProjectId, form]);

  function syncRepositoryOptions(keys: string[]) {
    setSelectedWorkspaceKeys(keys);
    setRepositoryOptions((current) =>
      keys.map((workspaceKey) => {
        const workspace = workspaces.find(
          (item) => item.workspaceKey === workspaceKey,
        );
        const existing = current.find(
          (item) => item.workspaceKey === workspaceKey,
        );
        return (
          existing ?? {
            workspaceKey,
            alias: workspace?.name || workspaceKey,
            role: "",
          }
        );
      }),
    );
  }

  async function nextFromBasics() {
    try {
      const values = await form.validateFields();
      setProjectBasics(values);
      setStep(1);
    } catch {
      // Form 会定位并显示字段错误。
    }
  }

  function nextFromRepositories() {
    if (!selectedWorkspaceKeys.length) {
      message.warning("请至少选择一个代码仓库");
      return;
    }
    setStep(2);
  }

  function nextFromVersion() {
    if (!initialVersionName.trim()) {
      message.warning("请填写版本名称");
      return;
    }
    setStep(3);
  }

  function projectKeyFor(projectName: string) {
    const name = projectName.trim();
    const previous = projectKeyAttemptRef.current;
    if (previous?.name === name) return previous.projectKey;

    const attempt = { name, projectKey: createProjectKey(name) };
    projectKeyAttemptRef.current = attempt;
    saveProjectKeyAttempt(attempt);
    return attempt.projectKey;
  }

  async function createProject() {
    try {
      const values = projectBasics;
      if (!values.name.trim()) {
        setStep(0);
        message.warning("请先填写项目名称");
        return;
      }
      if (!selectedWorkspaces.length) {
        setStep(1);
        message.warning("请至少选择一个代码仓库");
        return;
      }
      if (existingProject && unavailableRestoredWorkspaceKeys.length) {
        setStep(1);
        message.error(
          `已保存的代码仓库暂时无法读取：${unavailableRestoredWorkspaceKeys.join("、")}。请先在 Git 工作区恢复这些仓库，再继续登记版本。`,
        );
        return;
      }
      setSetupError(null);
      setCreating(true);
      const firstWorkspace = selectedWorkspaces[0];
      const project = existingProject
        ? existingProject
        : await knowledgeCatalogApi.upsertProject({
            projectKey: projectKeyFor(values.name),
            name: values.name.trim(),
            aliases: [],
            description: values.description?.trim() ?? "",
            gitWorkspaceKeys: selectedWorkspaces.map(
              (item) => item.workspaceKey,
            ),
            gitWorkspaceKey: firstWorkspace.workspaceKey,
            defaultBranch: workspaceDefaultBranch(firstWorkspace),
            enabled: true,
          } satisfies UpsertKnowledgeProjectInput);
      // 项目写入一旦成功就立即作为恢复锚点保存。即使下一步仓库绑定也失败，重试也只会
      // 从仓库绑定继续，不会再次创建项目。
      if (!existingProject) setExistingProject(project);
      const bindings = reusesSavedBindings
        ? restoredBindings
        : await knowledgeCatalogApi.replaceRepositoryBindings({
            projectId: project.id,
            repositories: selectedWorkspaces.map((workspace) => {
              const option = repositoryOptions.find(
                (item) => item.workspaceKey === workspace.workspaceKey,
              );
              return {
                workspaceKey: workspace.workspaceKey,
                alias: option?.alias.trim() || workspace.name,
                role: option?.role.trim() || null,
                defaultBranch: workspaceDefaultBranch(workspace),
                versionStrategy: "branch",
              };
            }),
          });
      // 每一步成功后即保留其结果。版本清单、来源登记或同步失败时，重试只继续未完成
      // 的步骤，绝不再次创建项目或替换已保存的仓库绑定。
      if (!reusesSavedBindings) {
        setRestoredBindings(bindings);
        setRestoredBindingWorkspaceKeys(
          bindings.map((binding) => binding.workspaceKey),
        );
      }
      let targetReleaseId = releaseId;
      if (targetReleaseId == null) {
        const manifest = await knowledgeCatalogApi.createProjectVersionManifest(
          {
            projectId: project.id,
            version: initialVersionName.trim(),
            repositories: bindings.map((binding) => {
              const workspace = selectedWorkspaces.find(
                (item) => item.workspaceKey === binding.workspaceKey,
              );
              return {
                repositoryBindingId: binding.id,
                refType: "branch",
                refName: versionBranchFor(
                  binding.workspaceKey,
                  workspace
                    ? workspaceDefaultBranch(workspace)
                    : binding.defaultBranch,
                ),
              };
            }),
          },
        );
        targetReleaseId = manifest.releaseId;
        setReleaseId(targetReleaseId);
      }
      const sourceInputs = selectedWorkspaces.map(
        (workspace) =>
          ({
            sourceKey: `project-${project.id}-workspace-${workspace.id}`,
            projectId: project.id,
            sourceType: "git_workspace",
            displayName: workspace.name,
            rootPath: workspace.repoPath,
            gitWorkspaceKey: workspace.workspaceKey,
            includeGlobs: [],
            excludeGlobs: [],
            // 仓库绑定的 `branch` 是版本清单引用类型；知识源沿用既有的
            // `git_ref` 契约，避免把两个领域的策略枚举混用而使首次同步被拒绝。
            versionStrategy: "git_ref",
            syncMode: "manual",
            allowRemoteEmbedding: false,
            enabled: true,
          }) satisfies Parameters<typeof knowledgeIngestionApi.upsertSource>[0],
      );
      // 后端批量登记刻意要求至少两个来源，以保证多仓库操作的原子性；
      // 单仓库必须调用对应的单条接口，避免项目和版本成功后首次同步永远失败。
      const sources =
        sourceInputs.length === 1
          ? [await knowledgeIngestionApi.upsertSource(sourceInputs[0])]
          : await knowledgeIngestionApi.upsertSourcesAtomically(sourceInputs);
      // 项目设置完成后，用户应可直接进入源码分析，而不必在另一套表单中重复登记同一
      // Git 工作区。代码源复用普通来源的稳定标识；远程 AI 分析默认可用，远程
      // Embedding 仍保持独立策略。
      await Promise.all(
        sourceInputs.map((source) =>
          knowledgeAnalysisApi.upsertCodeSource({
            source,
            includeUntracked: false,
            maxFileSizeBytes: 1_048_576,
            allowedLanguages: [
              "rust",
              "typescript",
              "javascript",
              "vue",
              "java",
              "sql",
            ],
            allowRemoteProcessing: true,
          }),
        ),
      );
      setCreatedProject(project);
      clearProjectKeyAttempt();
      projectKeyAttemptRef.current = null;
      setSyncSources(sources);
      const failed = await startInitialSync(sources, targetReleaseId);
      if (failed) {
        setSyncError(failed);
      } else {
        message.success(
          existingProject
            ? "项目版本已登记，首次同步已开始"
            : "项目已创建，首次同步已开始",
        );
      }
    } catch (error) {
      const errorMessage = getErrorMessage(error);
      setSetupError(errorMessage);
      message.error(errorMessage);
    } finally {
      setCreating(false);
    }
  }

  async function startInitialSync(
    sources: KnowledgeSource[],
    targetReleaseId: number,
  ): Promise<string | null> {
    const result = await Promise.allSettled(
      sources.map((source) => {
        const workspace = selectedWorkspaces.find(
          (item) => item.workspaceKey === source.gitWorkspaceKey,
        );
        return knowledgeIngestionApi.startSourceSync({
          sourceId: source.id,
          releaseId: targetReleaseId,
          gitRef: workspace
            ? versionBranchFor(
                workspace.workspaceKey,
                workspaceDefaultBranch(workspace),
              )
            : undefined,
        });
      }),
    );
    const failures = result.filter((item) => item.status === "rejected");
    return failures.length
      ? `${failures.length} 个仓库暂时无法开始同步，请重试。`
      : null;
  }

  async function retryInitialSync() {
    if (!releaseId || !syncSources.length) return;
    setCreating(true);
    try {
      const error = await startInitialSync(syncSources, releaseId);
      setSyncError(error);
      if (!error) message.success("首次同步已开始");
    } catch (error) {
      setSyncError(getErrorMessage(error));
    } finally {
      setCreating(false);
    }
  }

  if (createdProject) {
    return (
      <main className="w-full px-4 py-10 sm:px-6">
        <Result
          status="success"
          icon={<Check size={48} />}
          title="项目已准备好"
          subTitle={
            syncError
              ? "项目和版本已保存，但有仓库尚未开始同步。"
              : "已保存项目、代码仓库和初始版本，首次同步正在后台进行。"
          }
          extra={
            <Space wrap>
              {syncError ? (
                <Button
                  type="primary"
                  loading={creating}
                  onClick={() => void retryInitialSync()}
                >
                  重试同步
                </Button>
              ) : null}
              <Button
                type="primary"
                onClick={() =>
                  navigate(`/knowledge/projects/${createdProject.id}/overview`)
                }
              >
                进入项目
              </Button>
              <Button onClick={() => navigate("/knowledge/projects")}>
                项目列表
              </Button>
            </Space>
          }
        />
      </main>
    );
  }

  return (
    <main className="w-full px-4 py-6 sm:px-6">
      <Button
        type="link"
        className="!mb-4 !px-0"
        icon={<ArrowLeft size={16} />}
        onClick={() =>
          step === 0 ? navigate("/knowledge/projects") : setStep(step - 1)
        }
      >
        {step === 0 ? "返回项目列表" : "上一步"}
      </Button>
      <Title level={2} className="!mb-1">
        {existingProject ? "继续项目设置" : "创建项目"}
      </Title>
      <Paragraph type="secondary">只需四步，系统会自动保存推荐设置。</Paragraph>
      {existingProjectLoading ? (
        <Card className="mt-5" loading />
      ) : existingProjectError ? (
        <Alert
          className="mt-4"
          type="error"
          showIcon
          title="暂时无法继续设置"
          description={existingProjectError}
          action={
            <Button onClick={() => navigate("/knowledge/projects")}>
              返回项目列表
            </Button>
          }
        />
      ) : (
        <Card className="mt-5">
          <Form form={form} layout="vertical" requiredMark="optional">
            <Steps
              current={step}
              items={stepTitles.map((title) => ({ title }))}
            />
            <div className="mt-8">
              {step === 0 ? (
                <>
                  <Form.Item
                    label="项目名称"
                    name="name"
                    rules={[
                      {
                        required: true,
                        whitespace: true,
                        message: "请输入项目名称",
                      },
                    ]}
                  >
                    <Input
                      autoFocus
                      maxLength={100}
                      placeholder="例如：客户服务平台"
                    />
                  </Form.Item>
                  <Form.Item label="项目说明" name="description">
                    <Input.TextArea
                      rows={4}
                      maxLength={500}
                      showCount
                      placeholder="用一句话说明项目用途（可选）"
                    />
                  </Form.Item>
                  <Button
                    type="primary"
                    icon={<ArrowRight size={16} />}
                    iconPlacement="end"
                    onClick={() => void nextFromBasics()}
                  >
                    选择代码仓库
                  </Button>
                </>
              ) : null}

              {step === 1 ? (
                <Space orientation="vertical" size={16} className="w-full">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div>
                      <Text strong>选择代码仓库</Text>
                      <br />
                      <Text type="secondary">
                        选择要纳入同一项目的代码仓库，可多选。
                      </Text>
                    </div>
                    <Button
                      icon={<RefreshCw size={16} />}
                      loading={workspacesLoading}
                      onClick={() => void loadWorkspaces()}
                    >
                      刷新
                    </Button>
                  </div>
                  {workspacesError ? (
                    <Alert
                      type="error"
                      showIcon
                      title="代码仓库暂时无法读取"
                      description={workspacesError}
                      action={
                        <Button onClick={() => void loadWorkspaces()}>
                          重试
                        </Button>
                      }
                    />
                  ) : null}
                  {unavailableRestoredWorkspaceKeys.length ? (
                    <Alert
                      type="warning"
                      showIcon
                      title="部分已保存的代码仓库暂时无法读取"
                      description={`请先在“Git 工作区”恢复：${unavailableRestoredWorkspaceKeys.join("、")}。为了避免意外移除已有仓库，暂不能登记版本。`}
                    />
                  ) : null}
                  {reusesSavedBindings ? (
                    <Alert
                      type="info"
                      showIcon
                      title="已保留项目和代码仓库"
                      description="这些设置已经保存，本次只需登记缺失版本。"
                    />
                  ) : null}
                  <Select
                    mode="multiple"
                    showSearch
                    allowClear
                    loading={workspacesLoading}
                    value={selectedWorkspaceKeys}
                    placeholder="选择已登记的代码仓库"
                    optionFilterProp="label"
                    className="w-full"
                    onChange={syncRepositoryOptions}
                    disabled={reusesSavedBindings}
                    options={workspaces.map((workspace) => ({
                      value: workspace.workspaceKey,
                      label: `${workspace.name} · ${workspace.branch || "默认分支"}`,
                    }))}
                  />
                  {!workspacesLoading &&
                  !workspacesError &&
                  !workspaces.length ? (
                    <Alert
                      type="info"
                      showIcon
                      title="还没有可选的代码仓库"
                      description="请先在“Git 工作区”中登记仓库，再返回此处继续。"
                    />
                  ) : null}
                  {selectedWorkspaces.length ? (
                    <Collapse
                      items={[
                        {
                          key: "advanced",
                          label: "高级设置（可选）",
                          children: (
                            <Space
                              orientation="vertical"
                              className="w-full"
                              size={12}
                            >
                              <Text type="secondary">
                                默认使用仓库名称和当前分支。只有需要特殊显示名称或仓库职责时才修改。
                              </Text>
                              {repositoryOptions.map((option) => (
                                <div
                                  key={option.workspaceKey}
                                  className="grid gap-2 sm:grid-cols-2"
                                >
                                  <Input
                                    aria-label={`${option.alias}显示名称`}
                                    value={option.alias}
                                    placeholder="显示名称"
                                    onChange={(event) =>
                                      setRepositoryOptions((current) =>
                                        current.map((item) =>
                                          item.workspaceKey ===
                                          option.workspaceKey
                                            ? {
                                                ...item,
                                                alias: event.target.value,
                                              }
                                            : item,
                                        ),
                                      )
                                    }
                                  />
                                  <Input
                                    aria-label={`${option.alias}仓库职责`}
                                    value={option.role}
                                    placeholder="仓库职责（可选）"
                                    onChange={(event) =>
                                      setRepositoryOptions((current) =>
                                        current.map((item) =>
                                          item.workspaceKey ===
                                          option.workspaceKey
                                            ? {
                                                ...item,
                                                role: event.target.value,
                                              }
                                            : item,
                                        ),
                                      )
                                    }
                                  />
                                </div>
                              ))}
                            </Space>
                          ),
                        },
                      ]}
                    />
                  ) : null}
                  <Button
                    type="primary"
                    icon={<ArrowRight size={16} />}
                    iconPlacement="end"
                    onClick={nextFromRepositories}
                  >
                    选择初始版本
                  </Button>
                </Space>
              ) : null}

              {step === 2 ? (
                <Space orientation="vertical" size={18} className="w-full">
                  <div>
                    <Text strong>选择初始版本</Text>
                    <Paragraph type="secondary" className="!mb-0 !mt-1">
                      推荐使用每个仓库当前分支。系统会记录当时的版本，不会修改你的仓库。
                    </Paragraph>
                  </div>
                  <Form.Item label="版本名称" className="!mb-0">
                    <Input
                      value={initialVersionName}
                      maxLength={100}
                      onChange={(event) =>
                        setInitialVersionName(event.target.value)
                      }
                    />
                  </Form.Item>
                  <Card size="small" className="bg-[var(--bg-secondary)]">
                    <Text strong>使用当前分支（推荐）</Text>
                    <div className="mt-2 flex flex-wrap gap-2">
                      {selectedWorkspaces.map((workspace) => (
                        <Tag
                          key={workspace.workspaceKey}
                          icon={<GitBranch size={13} />}
                        >
                          {workspace.name} · {workspaceDefaultBranch(workspace)}
                        </Tag>
                      ))}
                    </div>
                  </Card>
                  <Button
                    type="primary"
                    icon={<ArrowRight size={16} />}
                    iconPlacement="end"
                    onClick={nextFromVersion}
                  >
                    查看同步内容
                  </Button>
                </Space>
              ) : null}

              {step === 3 ? (
                <Space orientation="vertical" size={18} className="w-full">
                  <div>
                    <Text strong>确认同步内容</Text>
                    <Paragraph type="secondary" className="!mb-0 !mt-1">
                      确认后会创建项目和版本，并开始读取所选仓库中的文档；不会修改
                      Git 仓库。
                    </Paragraph>
                  </div>
                  <DescriptionsForConfirmation
                    name={projectBasics.name}
                    description={projectBasics.description}
                    versionName={initialVersionName}
                    workspaces={selectedWorkspaces}
                  />
                  <Alert
                    type="info"
                    showIcon
                    title="创建后不会修改你的 Git 仓库"
                    description="系统只读取仓库信息，不会切换分支、提交、推送或执行仓库脚本。"
                  />
                  {setupError ? (
                    <Alert
                      type="error"
                      showIcon
                      title="版本登记暂未完成"
                      description={setupError}
                      action={
                        <Button
                          loading={creating}
                          onClick={() => void createProject()}
                        >
                          重试登记
                        </Button>
                      }
                    />
                  ) : null}
                  <Button
                    type="primary"
                    size="large"
                    icon={<FolderPlus size={17} />}
                    loading={creating}
                    onClick={() => void createProject()}
                  >
                    {existingProject
                      ? "登记版本并开始同步"
                      : "创建项目并开始同步"}
                  </Button>
                </Space>
              ) : null}
            </div>
          </Form>
        </Card>
      )}
    </main>
  );
}

function DescriptionsForConfirmation({
  name,
  description,
  versionName,
  workspaces,
}: {
  name?: string;
  description?: string;
  versionName: string;
  workspaces: GitWorkspace[];
}) {
  return (
    <Card size="small" className="bg-[var(--bg-secondary)]">
      <Space orientation="vertical" size={10} className="w-full">
        <div>
          <Text type="secondary">项目名称</Text>
          <br />
          <Text strong>{name}</Text>
        </div>
        {description ? (
          <div>
            <Text type="secondary">项目说明</Text>
            <br />
            <Text>{description}</Text>
          </div>
        ) : null}
        <div>
          <Text type="secondary">初始版本</Text>
          <br />
          <Text>{versionName}</Text>
        </div>
        <div>
          <Text type="secondary">代码仓库</Text>
          <div className="mt-2 flex flex-wrap gap-2">
            {workspaces.map((workspace) => (
              <Tag key={workspace.workspaceKey} icon={<GitBranch size={13} />}>
                {workspace.name} · {workspaceDefaultBranch(workspace)}
              </Tag>
            ))}
          </div>
        </div>
      </Space>
    </Card>
  );
}
