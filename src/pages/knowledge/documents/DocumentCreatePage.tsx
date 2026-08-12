import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  Alert,
  Button,
  Card,
  Checkbox,
  Collapse,
  Empty,
  Form,
  Input,
  Modal,
  Radio,
  Select,
  Skeleton,
  Space,
  Typography,
  message,
} from "antd";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ArrowLeft, FilePlus2, FileUp, FolderOpen, Upload } from "lucide-react";
import { getErrorMessage, hasTauriRuntime } from "@/lib/api";
import { aiProviderApi } from "@/lib/api/aiProvider";
import {
  knowledgeCatalogApi,
  knowledgeDocumentsApi,
  knowledgeIngestionApi,
} from "@/lib/api/knowledge-domain";
import type { KnowledgeProject, KnowledgeRelease } from "@/types";
import type { AiProvider } from "@/types/aiProvider";
import type {
  KnowledgeDocumentCommitResult,
  KnowledgeDocumentDraft,
  RestoreKnowledgeDocumentVersionToDraftResult,
} from "@/types/knowledge-domain/documents";
import type {
  PreparedKnowledgeUploadDirectory,
  PreparedKnowledgeUploadFile,
} from "@/types/knowledge-domain/ingestion";

const { Paragraph, Text, Title } = Typography;

type CreateMode = "manual" | "upload";
type VersionScope = "release" | "all";

interface ManualDraftValues {
  title: string;
  content: string;
  editorLabel?: string;
}

interface CommitDraftValues {
  versionLabel: string;
  commitMessage?: string;
}

interface DraftConflict {
  serverDraft: KnowledgeDocumentDraft;
  localValues: ManualDraftValues;
}

/**
 * 全新的文档入口只要求用户做一个选择：新写内容或上传已有文件。
 * 项目与版本均从工作台上下文继承；低频编辑者信息收纳到高级设置，避免暴露内部键。
 */
export default function DocumentCreatePage() {
  const navigate = useNavigate();
  const { projectId } = useParams();
  const [searchParams] = useSearchParams();
  const numericProjectId = Number(projectId);
  const restoreVersionId = Number(searchParams.get("restoreVersionId"));
  const shouldRestoreHistory =
    Number.isSafeInteger(restoreVersionId) && restoreVersionId > 0;
  const [project, setProject] = useState<KnowledgeProject | null>(null);
  const [releases, setReleases] = useState<KnowledgeRelease[]>([]);
  const [selectedReleaseId, setSelectedReleaseId] = useState<number | null>(
    null,
  );
  const [versionScope, setVersionScope] = useState<VersionScope>("release");
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [mode, setMode] = useState<CreateMode>("manual");
  const [savingDraft, setSavingDraft] = useState(false);
  const [savedDraft, setSavedDraft] = useState<KnowledgeDocumentDraft | null>(
    null,
  );
  const [draftIsCurrent, setDraftIsCurrent] = useState(false);
  const [draftConflict, setDraftConflict] = useState<DraftConflict | null>(
    null,
  );
  const [commitOpen, setCommitOpen] = useState(false);
  const [committing, setCommitting] = useState(false);
  const [commitResult, setCommitResult] =
    useState<KnowledgeDocumentCommitResult | null>(null);
  const [restoringHistory, setRestoringHistory] =
    useState(shouldRestoreHistory);
  const [historyRestoreError, setHistoryRestoreError] = useState<string | null>(
    null,
  );
  const [selectingFile, setSelectingFile] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [selectedFiles, setSelectedFiles] = useState<
    PreparedKnowledgeUploadFile[]
  >([]);
  const [selectedDirectory, setSelectedDirectory] =
    useState<PreparedKnowledgeUploadDirectory | null>(null);
  const [visionProviders, setVisionProviders] = useState<AiProvider[]>([]);
  const [loadingVisionProviders, setLoadingVisionProviders] = useState(false);
  const [allowRemoteOcr, setAllowRemoteOcr] = useState(false);
  const [ocrProviderKey, setOcrProviderKey] = useState<string | null>(null);
  const [form] = Form.useForm<ManualDraftValues>();
  const [commitForm] = Form.useForm<CommitDraftValues>();
  const contextRequestId = useRef(0);
  const historyRestoreRequestId = useRef(0);
  const loadedProjectId = useRef<number | null>(null);

  const selectedRelease = releases.find(
    (release) => release.id === selectedReleaseId,
  );
  const selectedFile = selectedFiles.length === 1 ? selectedFiles[0] : null;
  const selectedFileNeedsOcr =
    selectedDirectory == null &&
    selectedFile != null &&
    isRasterImageFile(selectedFile.displayName);
  const selectedFileIsUnsupportedSvg =
    selectedFile != null && isSvgFile(selectedFile.displayName);
  const selectedVisionProvider = visionProviders.find(
    (provider) => provider.key === ocrProviderKey,
  );

  const loadContext = useCallback(async () => {
    const requestId = ++contextRequestId.current;
    const isCurrentRequest = () => requestId === contextRequestId.current;
    if (!Number.isSafeInteger(numericProjectId) || numericProjectId < 1) {
      if (isCurrentRequest()) {
        setLoadError("项目地址无效");
        setLoading(false);
      }
      return;
    }
    setLoading(true);
    setLoadError(null);
    try {
      const projects = await knowledgeCatalogApi.listProjects({
        limit: 100,
        offset: 0,
      });
      const currentProject =
        projects.items.find((item) => item.id === numericProjectId) ?? null;
      if (!currentProject) {
        if (isCurrentRequest()) {
          setProject(null);
          setReleases([]);
          setSelectedReleaseId(null);
        }
        return;
      }
      const projectReleases =
        await knowledgeCatalogApi.listReleases(numericProjectId);
      if (!isCurrentRequest()) return;
      setProject(currentProject);
      setReleases(projectReleases);
      // 无版本项目不能提交“版本 ID 和跨版本范围均为空”的请求；默认切换到
      // 后端明确支持的全部版本范围，避免用户选完文件后卡在不可提交状态。
      setVersionScope(projectReleases.length ? "release" : "all");
      setSelectedReleaseId((current) =>
        current != null && projectReleases.some((item) => item.id === current)
          ? current
          : (projectReleases[0]?.id ?? null),
      );
    } catch (error) {
      if (isCurrentRequest()) setLoadError(getErrorMessage(error));
    } finally {
      if (isCurrentRequest()) setLoading(false);
    }
  }, [numericProjectId]);

  useEffect(() => {
    setProject(null);
    setReleases([]);
    setSelectedReleaseId(null);
    setVersionScope("release");
    setMode("manual");
    setSavingDraft(false);
    setSavedDraft(null);
    setDraftIsCurrent(false);
    setDraftConflict(null);
    setCommitOpen(false);
    setCommitting(false);
    setCommitResult(null);
    setRestoringHistory(shouldRestoreHistory);
    setHistoryRestoreError(null);
    setSelectingFile(false);
    setUploading(false);
    setSelectedFiles([]);
    setSelectedDirectory(null);
    setVisionProviders([]);
    setLoadingVisionProviders(false);
    setAllowRemoteOcr(false);
    setOcrProviderKey(null);
    void loadContext();
    return () => {
      // 路由切换或组件卸载时立即使旧项目的异步响应和文件准备结果失效。
      contextRequestId.current += 1;
      historyRestoreRequestId.current += 1;
    };
  }, [loadContext, shouldRestoreHistory]);

  useEffect(() => {
    if (loading || !project) return;
    if (
      loadedProjectId.current != null &&
      loadedProjectId.current !== project.id
    ) {
      // 新项目的表单不能复用旧项目的草稿字段；此时 Form 已挂载，重置不会触发
      // Ant Design 的未连接表单警告。
      form.resetFields();
      commitForm.resetFields();
    }
    loadedProjectId.current = project.id;
  }, [commitForm, form, loading, project]);

  useEffect(() => {
    if (!shouldRestoreHistory || !project) {
      if (!shouldRestoreHistory) setRestoringHistory(false);
      return;
    }
    const requestId = ++historyRestoreRequestId.current;
    setRestoringHistory(true);
    setHistoryRestoreError(null);
    setMode("manual");
    void knowledgeDocumentsApi
      .restoreVersionToDraft({ sourceVersionId: restoreVersionId })
      .then((result: RestoreKnowledgeDocumentVersionToDraftResult) => {
        if (requestId !== historyRestoreRequestId.current) return;
        if (result.draft.projectId !== project.id) {
          throw new Error("历史文档不属于当前项目，不能在这里恢复。");
        }
        if (result.conflict) {
          setDraftConflict({
            serverDraft: result.draft,
            localValues: {
              title: result.draft.title,
              content: result.draft.content,
              editorLabel: result.draft.editorLabel || undefined,
            },
          });
          setSavedDraft(null);
          setDraftIsCurrent(false);
          return;
        }
        form.setFieldsValue({
          title: result.draft.title,
          content: result.draft.content,
          editorLabel: result.draft.editorLabel || undefined,
        });
        if (result.sourceVersion.releaseId != null) {
          setVersionScope("release");
          setSelectedReleaseId(result.sourceVersion.releaseId);
        } else {
          setVersionScope("all");
        }
        setSavedDraft(result.draft);
        setDraftIsCurrent(true);
        setDraftConflict(null);
      })
      .catch((error: unknown) => {
        if (requestId === historyRestoreRequestId.current) {
          setHistoryRestoreError(getErrorMessage(error));
        }
      })
      .finally(() => {
        if (requestId === historyRestoreRequestId.current) {
          setRestoringHistory(false);
        }
      });
    return () => {
      historyRestoreRequestId.current += 1;
    };
  }, [form, project, restoreVersionId, shouldRestoreHistory]);

  async function saveDraft(values: ManualDraftValues) {
    if (!project || draftConflict) return;
    const contextId = contextRequestId.current;
    setSavingDraft(true);
    try {
      const result = await knowledgeDocumentsApi.saveDraft({
        draftId: savedDraft?.id ?? null,
        revision: savedDraft?.revision ?? null,
        projectId: project.id,
        title: values.title.trim(),
        content: values.content,
        docType: "markdown",
        editorLabel: values.editorLabel?.trim() || null,
      });
      if (contextId !== contextRequestId.current) return;
      if (result.conflict) {
        setDraftConflict({ serverDraft: result.draft, localValues: values });
        setSavedDraft(null);
        setDraftIsCurrent(false);
        message.warning("草稿已被其他编辑者更新，请先处理冲突。");
        return;
      }
      setSavedDraft(result.draft);
      setDraftIsCurrent(true);
      setDraftConflict(null);
      setCommitResult(null);
      message.success("草稿已保存，可以继续编辑或提交为正式版本。");
      form.setFieldsValue({
        title: result.draft.title,
        content: result.draft.content,
      });
    } catch (error) {
      if (contextId === contextRequestId.current) {
        message.error(getErrorMessage(error));
      }
    } finally {
      if (contextId === contextRequestId.current) setSavingDraft(false);
    }
  }

  function openCommitDialog() {
    if (
      !savedDraft ||
      !draftIsCurrent ||
      (versionScope === "release" && !selectedRelease)
    ) {
      return;
    }
    commitForm.setFieldsValue({
      versionLabel: selectedRelease?.version ?? "全部版本",
      commitMessage: "",
    });
    setCommitOpen(true);
  }

  async function commitDraft(values: CommitDraftValues) {
    if (
      !savedDraft ||
      (versionScope === "release" && selectedReleaseId == null)
    ) {
      return;
    }
    const contextId = contextRequestId.current;
    setCommitting(true);
    try {
      const result = await knowledgeDocumentsApi.commitDraft({
        draftId: savedDraft.id,
        revision: savedDraft.revision,
        versionLabel: values.versionLabel.trim(),
        projectVersionId: versionScope === "release" ? selectedReleaseId : null,
        crossVersionScope:
          versionScope === "all" ? "project_all_versions" : null,
        commitMessage: values.commitMessage?.trim() || null,
      });
      if (contextId !== contextRequestId.current) return;
      setCommitResult(result);
      setSavedDraft(null);
      setDraftIsCurrent(false);
      setCommitOpen(false);
      message.success("已提交为正式版本，索引已排队处理。");
    } catch (error) {
      if (contextId === contextRequestId.current) {
        message.error(getErrorMessage(error));
      }
    } finally {
      if (contextId === contextRequestId.current) setCommitting(false);
    }
  }

  function startAnotherManualDocument() {
    if (shouldRestoreHistory) {
      navigate(`/knowledge/projects/${numericProjectId}/documents/new`);
      return;
    }
    form.resetFields();
    setSavedDraft(null);
    setDraftIsCurrent(false);
    setDraftConflict(null);
    setCommitResult(null);
  }

  function loadServerDraft() {
    if (!draftConflict) return;
    form.setFieldsValue({
      title: draftConflict.serverDraft.title,
      content: draftConflict.serverDraft.content,
      editorLabel: draftConflict.serverDraft.editorLabel || undefined,
    });
    setSavedDraft(draftConflict.serverDraft);
    setDraftIsCurrent(true);
    setDraftConflict(null);
    message.info("已加载服务器草稿。请手工合并本地修改后再保存。");
  }

  async function chooseFile() {
    if (!hasTauriRuntime()) {
      message.error("选择上传文件需要在 Tauri 桌面端运行。");
      return;
    }
    const contextId = contextRequestId.current;
    setSelectingFile(true);
    try {
      const selected = await openDialog({
        multiple: true,
        directory: false,
        filters: [
          {
            name: "项目文档",
            extensions: [
              "md",
              "mdx",
              "txt",
              "json",
              "yaml",
              "yml",
              "css",
              "js",
              "mjs",
              "html",
              "htm",
              "docx",
              "xlsx",
              "pptx",
              "doc",
              "xls",
              "ppt",
              "pdf",
              "png",
              "jpg",
              "jpeg",
              "webp",
              "gif",
              "svg",
            ],
          },
        ],
      });
      if (!selected) return;
      if (contextId !== contextRequestId.current) return;
      const selectedPaths = Array.isArray(selected) ? selected : [selected];
      const preparedResults = await Promise.all(
        selectedPaths.map(async (selectedPath) => {
          try {
            return {
              result: await knowledgeIngestionApi.prepareUploadFile({
                selectedPath,
              }),
              error: null,
            };
          } catch (error) {
            return { result: null, error: getErrorMessage(error) };
          }
        }),
      );
      if (contextId !== contextRequestId.current) return;
      const preparedFiles = preparedResults.flatMap(({ result }) =>
        result ? [result] : [],
      );
      const failures = preparedResults.filter(({ error }) => error != null);
      setSelectedDirectory(null);
      setSelectedFiles(preparedFiles);
      setAllowRemoteOcr(false);
      setOcrProviderKey(null);
      setVisionProviders([]);
      if (failures.length) {
        message.warning(
          `有 ${failures.length} 个文件未通过安全检查，已保留其余 ${preparedFiles.length} 个文件。`,
        );
      }
      if (!preparedFiles.length) return;
      if (
        preparedFiles.length === 1 &&
        isRasterImageFile(preparedFiles[0].displayName)
      ) {
        setLoadingVisionProviders(true);
        try {
          const providers = await aiProviderApi.list();
          if (contextId !== contextRequestId.current) return;
          setVisionProviders(
            providers.filter(
              (provider) =>
                provider.enabled &&
                provider.status === "configured" &&
                provider.capabilities.some(
                  (capability) => capability.trim().toLowerCase() === "vision",
                ),
            ),
          );
        } catch (error) {
          if (contextId === contextRequestId.current) {
            message.error(`无法读取视觉识别服务：${getErrorMessage(error)}`);
          }
        } finally {
          if (contextId === contextRequestId.current) {
            setLoadingVisionProviders(false);
          }
        }
      }
    } catch (error) {
      if (contextId === contextRequestId.current) {
        message.error(getErrorMessage(error));
      }
    } finally {
      if (contextId === contextRequestId.current) setSelectingFile(false);
    }
  }

  async function chooseDirectory() {
    if (!hasTauriRuntime()) {
      message.error("选择上传文件夹需要在 Tauri 桌面端运行。");
      return;
    }
    const contextId = contextRequestId.current;
    setSelectingFile(true);
    try {
      const selected = await openDialog({
        multiple: false,
        directory: true,
      });
      if (!selected || Array.isArray(selected)) return;
      if (contextId !== contextRequestId.current) return;
      const prepared = await knowledgeIngestionApi.prepareUploadDirectory({
        selectedPath: selected,
      });
      if (contextId !== contextRequestId.current) return;
      setSelectedDirectory(prepared);
      setSelectedFiles(prepared.files);
      setAllowRemoteOcr(false);
      setOcrProviderKey(null);
      setVisionProviders([]);
      if (prepared.skippedCount > 0) {
        message.warning(
          `文件夹中有 ${prepared.skippedCount} 个不支持或未通过校验的文件，已跳过；其余 ${prepared.files.length} 个文件已准备完成。`,
        );
      }
    } catch (error) {
      if (contextId === contextRequestId.current) {
        message.error(getErrorMessage(error));
      }
    } finally {
      if (contextId === contextRequestId.current) setSelectingFile(false);
    }
  }

  async function startUpload() {
    if (
      !project ||
      !selectedFiles.length ||
      (versionScope === "release" && selectedReleaseId == null)
    ) {
      return;
    }
    const contextId = contextRequestId.current;
    setUploading(true);
    try {
      const commonInput = {
        projectId: project.id,
        projectVersionId: versionScope === "release" ? selectedReleaseId : null,
        crossVersionScope:
          versionScope === "all" ? ("project_all_versions" as const) : null,
      };
      if (selectedFiles.length === 1) {
        await knowledgeIngestionApi.createDocumentUpload({
          ...commonInput,
          fileHandle: selectedFiles[0].fileHandle,
          displayName: selectedFiles[0].displayName,
          ...(selectedDirectory
            ? { sourceFolderName: selectedDirectory.directoryName }
            : {}),
          ...(selectedFileNeedsOcr
            ? {
                allowRemoteOcr,
                ocrProviderKey: allowRemoteOcr ? ocrProviderKey : null,
              }
            : {}),
        });
        if (contextId !== contextRequestId.current) return;
        message.success("文件已加入处理队列，完成后会显示在项目文档中。");
      } else {
        const result = await knowledgeIngestionApi.createDocumentUploadBatch({
          ...commonInput,
          ...(selectedDirectory
            ? { sourceFolderName: selectedDirectory.directoryName }
            : {}),
          files: selectedFiles.map((file) => ({
            fileHandle: file.fileHandle,
            displayName: file.displayName,
            // 批量上传默认只在本机处理图片；逐张上传时才允许显式外发授权。
            allowRemoteOcr: false,
            ocrProviderKey: null,
          })),
        });
        if (contextId !== contextRequestId.current) return;
        const failedCount = result.items.filter(
          (item) => item.errorMessage != null,
        ).length;
        if (failedCount) {
          message.warning(
            `${result.items.length - failedCount} 个文件已加入处理队列，${failedCount} 个文件未能加入。`,
          );
        } else {
          message.success(`${result.items.length} 个文件已加入处理队列。`);
        }
      }
      setSelectedFiles([]);
      setSelectedDirectory(null);
      setAllowRemoteOcr(false);
      setOcrProviderKey(null);
      setVisionProviders([]);
    } catch (error) {
      if (contextId === contextRequestId.current) {
        message.error(getErrorMessage(error));
      }
    } finally {
      if (contextId === contextRequestId.current) setUploading(false);
    }
  }

  if (loading) {
    return <Skeleton active className="mt-8 w-full px-6" />;
  }

  if (loadError) {
    return (
      <main className="mt-8 w-full px-6">
        <Alert
          type="error"
          showIcon
          title="无法打开文档新增页"
          description={loadError}
          action={<Button onClick={() => void loadContext()}>重试</Button>}
        />
      </main>
    );
  }

  if (!project) {
    return (
      <main className="mt-8 w-full px-6">
        <Empty description="没有找到这个项目">
          <Button
            type="primary"
            onClick={() => navigate("/knowledge/projects")}
          >
            返回项目列表
          </Button>
        </Empty>
      </main>
    );
  }

  return (
    <main className="w-full px-4 py-6 sm:px-6">
      <Button
        type="link"
        className="!mb-4 !px-0"
        icon={<ArrowLeft size={16} />}
        onClick={() => navigate(`/knowledge/projects/${project.id}/overview`)}
      >
        返回项目
      </Button>
      <Title level={2} className="!mb-1">
        {shouldRestoreHistory ? "恢复并编辑文档" : "添加文档"}
      </Title>
      <Paragraph type="secondary" className="!mb-6">
        {shouldRestoreHistory
          ? "历史版本会先复制为新草稿；编辑并提交后才会创建新的正式版本，原历史不会被修改。"
          : "选择新写内容或上传已有文件。项目已自动选好，请明确关联版本或选择适用全部版本。"}
      </Paragraph>

      {restoringHistory ? (
        <Alert
          className="mb-4"
          type="info"
          showIcon
          title="正在准备历史草稿"
          description="将历史正文复制为可编辑草稿，不会修改原版本。"
        />
      ) : null}
      {historyRestoreError ? (
        <Alert
          className="mb-4"
          type="error"
          showIcon
          title="无法恢复历史版本"
          description={historyRestoreError}
        />
      ) : null}

      <Card className="mb-4">
        <Text strong>适用范围</Text>
        <Radio.Group
          className="mt-2"
          value={versionScope}
          onChange={(event) =>
            setVersionScope(event.target.value as VersionScope)
          }
        >
          <Radio value="release">关联一个项目版本</Radio>
          <Radio value="all">适用于全部版本</Radio>
        </Radio.Group>
        {versionScope === "release" ? (
          <label className="mt-3 block" htmlFor="document-project-version">
            <Select
              id="document-project-version"
              className="w-full"
              aria-label="关联版本"
              placeholder="请选择项目版本"
              value={selectedReleaseId}
              onChange={(value: number | undefined) =>
                setSelectedReleaseId(value ?? null)
              }
              options={releases.map((release) => ({
                value: release.id,
                label: release.version,
              }))}
            />
          </label>
        ) : (
          <Text type="secondary" className="mt-2 block">
            此文档会作为跨版本资料保存，不会被自动归入某个最新版本。
          </Text>
        )}
        {!releases.length ? (
          <Text type="secondary" className="mt-2 block">
            当前项目尚无版本。可选择“适用于全部版本”，或先登记项目版本。
          </Text>
        ) : null}
      </Card>

      <Radio.Group
        className="mb-4 grid w-full grid-cols-2 gap-3"
        value={mode}
        disabled={shouldRestoreHistory}
        onChange={(event) => setMode(event.target.value as CreateMode)}
      >
        <Radio.Button
          value="manual"
          className="!h-auto !rounded-lg !px-4 !py-3"
        >
          <Space>
            <FilePlus2 size={18} />
            新写文档
          </Space>
        </Radio.Button>
        <Radio.Button
          value="upload"
          className="!h-auto !rounded-lg !px-4 !py-3"
        >
          <Space>
            <FileUp size={18} />
            上传文件
          </Space>
        </Radio.Button>
      </Radio.Group>

      {mode === "manual" ? (
        <Card title={shouldRestoreHistory ? "历史草稿" : "新写文档"}>
          <Form
            form={form}
            layout="vertical"
            preserve
            disabled={savingDraft || restoringHistory || draftConflict != null}
            onFinish={(values) => void saveDraft(values)}
            onValuesChange={() => {
              if (savedDraft) setDraftIsCurrent(false);
            }}
          >
            <Form.Item
              name="title"
              label="文档标题"
              rules={[
                { required: true, whitespace: true, message: "请输入文档标题" },
                { max: 200, message: "文档标题不能超过 200 个字符" },
              ]}
            >
              <Input
                autoFocus
                maxLength={200}
                placeholder="例如：退款审批说明"
              />
            </Form.Item>
            <Form.Item
              name="content"
              label="文档内容"
              rules={[{ required: true, message: "请输入文档内容" }]}
            >
              <Input.TextArea rows={12} placeholder="支持 Markdown 书写方式" />
            </Form.Item>
            <Text type="secondary" className="-mt-2 mb-4 block">
              保存草稿后，提交时会确认关联版本。
            </Text>
            <Collapse
              className="mb-4"
              items={[
                {
                  key: "advanced",
                  label: "高级设置",
                  children: (
                    <Form.Item
                      name="editorLabel"
                      label="编辑者名称"
                      className="!mb-0"
                    >
                      <Input maxLength={80} placeholder="默认使用本地用户" />
                    </Form.Item>
                  ),
                },
              ]}
            />
            <Button type="primary" htmlType="submit" loading={savingDraft}>
              保存草稿
            </Button>
          </Form>
        </Card>
      ) : (
        <Card title="上传文件">
          <Space orientation="vertical" size={14} className="w-full">
            <Text>
              支持常见文档、Office、图片和 HTML 原型。HTML 原型可以选择包含
              index.html、CSS、JS
              和图片资源的文件夹，系统会递归准备受支持的文件并批量处理；不会执行
              HTML 或脚本。 旧版 DOC/XLS/PPT 请先另存为新版格式。
            </Text>
            {selectedFiles.length ? (
              <Alert
                type="success"
                showIcon
                title={
                  selectedDirectory
                    ? "已选择文件夹：" + selectedDirectory.directoryName
                    : selectedFiles.length === 1
                      ? selectedFiles[0].displayName
                      : `已选择 ${selectedFiles.length} 个文件`
                }
                description={
                  selectedDirectory ? (
                    <Space orientation="vertical" size={2}>
                      <Text>
                        {selectedDirectory.files.length} 个文件，合计{" "}
                        {formatFileSize(selectedDirectory.totalSizeBytes)}
                      </Text>
                      {selectedDirectory.skippedCount > 0 ? (
                        <Text type="secondary">
                          已跳过 {selectedDirectory.skippedCount}{" "}
                          个不支持或未通过校验的文件
                        </Text>
                      ) : null}
                    </Space>
                  ) : selectedFiles.length === 1 ? (
                    `已选择 ${formatFileSize(selectedFiles[0].sizeBytes)}`
                  ) : (
                    selectedFiles
                      .map(
                        (file) =>
                          `${file.displayName}（${formatFileSize(file.sizeBytes)}）`,
                      )
                      .join("；")
                  )
                }
                action={
                  <Button
                    aria-label="移除"
                    onClick={() => {
                      setSelectedFiles([]);
                      setSelectedDirectory(null);
                      setAllowRemoteOcr(false);
                      setOcrProviderKey(null);
                      setVisionProviders([]);
                    }}
                  >
                    移除
                  </Button>
                }
              />
            ) : (
              <Alert
                type="info"
                showIcon
                title="还没有选择文件"
                description="选择一个文件后，系统会自动推断标题和类型。"
              />
            )}
            {selectedFileNeedsOcr ? (
              <Alert
                type="warning"
                showIcon
                title="可选远程文字识别"
                description={
                  <Space orientation="vertical" size={10} className="w-full">
                    <Text>
                      默认会优先在本机识别图片文字，不会上传图片；本机暂不可用时，图片仍会安全保存，并可按标题和图片元数据搜索。
                      只有勾选下方授权后，才会发送图片至所选服务识别文字。
                    </Text>
                    {!loadingVisionProviders && !visionProviders.length ? (
                      <Text type="secondary">
                        当前没有可用的远程视觉识别服务，仍会优先使用本机文字识别；如需远程识别，请稍后在
                        AI 服务中配置并测试服务。
                      </Text>
                    ) : null}
                    <label className="block" htmlFor="document-ocr-provider">
                      <Text strong>视觉识别服务</Text>
                      <Select
                        id="document-ocr-provider"
                        className="mt-2 w-full"
                        aria-label="视觉识别服务"
                        loading={loadingVisionProviders}
                        disabled={
                          loadingVisionProviders || !visionProviders.length
                        }
                        placeholder="选择已配置的服务"
                        value={ocrProviderKey}
                        onChange={(value: string | undefined) =>
                          setOcrProviderKey(value ?? null)
                        }
                        options={visionProviders.map((provider) => ({
                          value: provider.key,
                          label: `${provider.name}（${provider.defaultModel}）`,
                        }))}
                      />
                    </label>
                    <Checkbox
                      checked={allowRemoteOcr}
                      disabled={!selectedVisionProvider}
                      onChange={(event) =>
                        setAllowRemoteOcr(event.target.checked)
                      }
                    >
                      我同意将本图片发送至所选服务进行文字识别
                    </Checkbox>
                  </Space>
                }
              />
            ) : null}
            {selectedFiles.length > 1 ? (
              <Alert
                type="info"
                showIcon
                title="批量上传将优先在本机处理图片"
                description="为避免一次外发多份资料，批量上传不会使用远程文字识别；如需远程识别，请单独上传该图片并明确授权。"
              />
            ) : null}
            {selectedFileIsUnsupportedSvg ? (
              <Alert
                type="error"
                showIcon
                title="SVG 将以安全元数据方式保存"
                description="SVG 不会发送至远程识别服务，当前也不会直接预览其原始内容；可转换为 PNG 或 JPEG 后获得图片预览。"
              />
            ) : null}
            <Space wrap>
              <Button
                icon={<Upload size={16} />}
                loading={selectingFile}
                onClick={() => void chooseFile()}
              >
                选择文件
              </Button>
              <Button
                icon={<FolderOpen size={16} />}
                loading={selectingFile}
                onClick={() => void chooseDirectory()}
              >
                选择文件夹
              </Button>
              <Button
                type="primary"
                disabled={
                  !selectedFiles.length ||
                  (versionScope === "release" && selectedReleaseId == null)
                }
                loading={uploading}
                onClick={() => void startUpload()}
              >
                开始上传
              </Button>
            </Space>
          </Space>
        </Card>
      )}

      {mode === "manual" && draftConflict ? (
        <Card className="mt-4" title="处理草稿冲突">
          <Space orientation="vertical" size={12} className="w-full">
            <Alert
              type="warning"
              showIcon
              title="草稿已被其他编辑者更新"
              description="为保护双方内容，当前草稿不能继续保存或提交。请先对比内容，复制需要保留的本地修改，再加载服务器草稿并手工合并。"
            />
            <Collapse
              items={[
                {
                  key: "local",
                  label: "本地未保存内容",
                  children: (
                    <Space orientation="vertical" size={8} className="w-full">
                      <Text>标题：{draftConflict.localValues.title}</Text>
                      <Input.TextArea
                        aria-label="本地未保存内容"
                        readOnly
                        rows={8}
                        value={draftConflict.localValues.content}
                      />
                    </Space>
                  ),
                },
                {
                  key: "server",
                  label: "服务器当前草稿",
                  children: (
                    <Space orientation="vertical" size={8} className="w-full">
                      <Text>标题：{draftConflict.serverDraft.title}</Text>
                      <Input.TextArea
                        aria-label="服务器当前草稿"
                        readOnly
                        rows={8}
                        value={draftConflict.serverDraft.content}
                      />
                    </Space>
                  ),
                },
              ]}
            />
            <Button type="primary" onClick={loadServerDraft}>
              加载服务器草稿
            </Button>
          </Space>
        </Card>
      ) : null}

      {mode === "manual" && savedDraft ? (
        <Card className="mt-4" title="提交正式版本">
          <Space orientation="vertical" size={12} className="w-full">
            {draftIsCurrent ? (
              <Alert
                type="success"
                showIcon
                title="草稿已保存"
                description="提交后会创建不可变文档版本，并将索引任务加入队列。"
              />
            ) : (
              <Alert
                type="warning"
                showIcon
                title="草稿有未保存的修改"
                description="请先保存草稿，再提交正式版本。"
              />
            )}
            {versionScope === "release" && !selectedRelease ? (
              <Alert
                type="info"
                showIcon
                title="请先选择关联版本"
                description="也可选择“适用于全部版本”，但不能让文档处于未绑定状态。"
              />
            ) : null}
            <Button
              type="primary"
              disabled={
                !draftIsCurrent ||
                (versionScope === "release" && !selectedRelease)
              }
              onClick={openCommitDialog}
            >
              提交为正式版本
            </Button>
          </Space>
        </Card>
      ) : null}

      {mode === "manual" && commitResult ? (
        <Alert
          className="mt-4"
          type="success"
          showIcon
          title="正式版本已创建"
          description="索引已排队处理；处理完成前，该文档不会出现在搜索结果中。"
          action={
            <Space wrap>
              <Button onClick={startAnotherManualDocument}>继续添加</Button>
              <Button
                type="primary"
                onClick={() =>
                  navigate(`/knowledge/projects/${project.id}/overview`)
                }
              >
                返回项目
              </Button>
            </Space>
          }
        />
      ) : null}

      <Modal
        title="提交为正式版本"
        open={commitOpen}
        okText="确认提交"
        cancelText="继续编辑"
        confirmLoading={committing}
        onOk={() => commitForm.submit()}
        onCancel={() => !committing && setCommitOpen(false)}
        mask={{ closable: false }}
        destroyOnHidden
      >
        <Form
          form={commitForm}
          layout="vertical"
          preserve={false}
          onFinish={(values) => void commitDraft(values)}
        >
          <Alert
            className="mb-4"
            type="info"
            showIcon
            title={
              versionScope === "all"
                ? "适用于项目全部版本"
                : `将关联项目版本：${selectedRelease?.version ?? "未选择"}`
            }
            description={
              versionScope === "all"
                ? "提交后该文档会纳入项目全部版本的资料范围。"
                : "提交后会保留该版本的内容历史，不能直接覆盖。"
            }
          />
          <Form.Item
            name="versionLabel"
            label="文档版本名称"
            rules={[
              {
                required: true,
                whitespace: true,
                message: "请输入文档版本名称",
              },
              { max: 80, message: "文档版本名称不能超过 80 个字符" },
            ]}
          >
            <Input maxLength={80} autoFocus />
          </Form.Item>
          <Form.Item
            name="commitMessage"
            label="提交说明"
            rules={[{ max: 500, message: "提交说明不能超过 500 个字符" }]}
          >
            <Input.TextArea
              rows={3}
              maxLength={500}
              placeholder="例如：补充退款审批规则"
            />
          </Form.Item>
        </Form>
      </Modal>
    </main>
  );
}

function isRasterImageFile(fileName: string) {
  return /\.(png|jpe?g|webp|gif)$/i.test(fileName.trim());
}

function isSvgFile(fileName: string) {
  return /\.svg$/i.test(fileName.trim());
}

function formatFileSize(sizeBytes: number) {
  if (sizeBytes < 1024) return `${sizeBytes} B`;
  if (sizeBytes < 1024 * 1024) return `${Math.ceil(sizeBytes / 1024)} KB`;
  return `${(sizeBytes / (1024 * 1024)).toFixed(1)} MB`;
}
