import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Alert,
  Button,
  Card,
  Empty,
  List,
  Popconfirm,
  Progress,
  Select,
  Skeleton,
  Space,
  Tag,
  Typography,
} from "antd";
import {
  ArrowLeft,
  GitBranch,
  RefreshCw,
  Settings2,
  ShieldCheck,
  Wrench,
} from "lucide-react";
import { getErrorMessage, hasTauriRuntime } from "@/lib/api/client";
import {
  knowledgeCatalogApi,
  knowledgeJobsApi,
} from "@/lib/api/knowledge-domain";
import type {
  KnowledgeJob,
  KnowledgeJobProgress,
  KnowledgeProject,
  KnowledgeRelease,
} from "@/types";
import type {
  KnowledgeProjectVersionCompleteness,
  KnowledgeProjectVersionManifestResult,
} from "@/types/knowledge-domain/catalog";

const { Paragraph, Text, Title } = Typography;

function isTerminalJobStatus(status: string) {
  return ["completed", "failed", "cancelled", "interrupted"].includes(status);
}

function toBackfillProgress(job: KnowledgeJob): KnowledgeJobProgress {
  return {
    jobKey: job.jobKey,
    status: job.status,
    stage: String(job.checkpoint.stage ?? job.status),
    current: job.progressCurrent,
    total: job.progressTotal,
    message: job.message,
    canCancel: job.status === "queued" || job.status === "running",
    error: job.error
      ? {
          code: `KNOWLEDGE_JOB_${job.status.toUpperCase()}`,
          message: job.error,
          stage: String(job.checkpoint.stage ?? job.status),
          sourceKey: "",
          retryable: job.status === "failed" || job.status === "interrupted",
          sanitizedDetails: {},
        }
      : null,
  };
}

/**
 * 版本页按“选择版本 → 查看仓库清单 → 查看处理进度”组织。Commit SHA 与逐仓库规则属于
 * 高级事实，默认只显示普通用户能据此继续操作的状态与下一步。
 */
export default function ProjectVersionsPage() {
  const navigate = useNavigate();
  const { projectId } = useParams();
  const numericProjectId = Number(projectId);
  const [project, setProject] = useState<KnowledgeProject | null>(null);
  const [releases, setReleases] = useState<KnowledgeRelease[]>([]);
  const [selectedReleaseId, setSelectedReleaseId] = useState<number | null>(
    null,
  );
  const [manifest, setManifest] =
    useState<KnowledgeProjectVersionManifestResult | null>(null);
  const [completeness, setCompleteness] =
    useState<KnowledgeProjectVersionCompleteness | null>(null);
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [backfilling, setBackfilling] = useState(false);
  const [backfillJob, setBackfillJob] = useState<KnowledgeJob | null>(null);
  const [backfillProgress, setBackfillProgress] =
    useState<KnowledgeJobProgress | null>(null);
  const [backfillError, setBackfillError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const requestId = useRef(0);
  const hasDocumentProcessingGaps =
    completeness?.stages.some(
      (stage) =>
        (stage.stage === "parsing" || stage.stage === "indexing") &&
        stage.status !== "ready",
    ) ?? false;

  const load = useCallback(async () => {
    const currentRequestId = ++requestId.current;
    if (!Number.isSafeInteger(numericProjectId) || numericProjectId < 1) {
      setError("项目地址无效");
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const [projects, projectReleases] = await Promise.all([
        knowledgeCatalogApi.listProjects({
          projectId: numericProjectId,
          limit: 1,
          offset: 0,
        }),
        knowledgeCatalogApi.listReleases(numericProjectId),
      ]);
      if (currentRequestId !== requestId.current) return;
      setProject(projects.items[0] ?? null);
      setReleases(projectReleases);
      setSelectedReleaseId((current) =>
        current != null && projectReleases.some((item) => item.id === current)
          ? current
          : (projectReleases[0]?.id ?? null),
      );
    } catch (cause) {
      if (currentRequestId === requestId.current) {
        setError(getErrorMessage(cause));
      }
    } finally {
      if (currentRequestId === requestId.current) setLoading(false);
    }
  }, [numericProjectId]);

  const loadVersion = useCallback(async (releaseId: number | null) => {
    const currentRequestId = ++requestId.current;
    if (releaseId == null) {
      setManifest(null);
      setCompleteness(null);
      return;
    }
    setDetailLoading(true);
    setError(null);
    try {
      const [nextManifest, nextCompleteness] = await Promise.all([
        knowledgeCatalogApi.getProjectVersionManifest(releaseId),
        knowledgeCatalogApi.getProjectVersionCompleteness(releaseId),
      ]);
      if (currentRequestId !== requestId.current) return;
      setManifest(nextManifest);
      setCompleteness(nextCompleteness);
    } catch (cause) {
      if (currentRequestId === requestId.current) {
        setError(getErrorMessage(cause));
        setManifest(null);
        setCompleteness(null);
      }
    } finally {
      if (currentRequestId === requestId.current) setDetailLoading(false);
    }
  }, []);

  const startBackfill = useCallback(async () => {
    if (selectedReleaseId == null) return;
    setBackfilling(true);
    setBackfillError(null);
    try {
      const job = await knowledgeCatalogApi.startProjectVersionBackfill({
        releaseId: selectedReleaseId,
      });
      setBackfillJob(job);
      setBackfillProgress(toBackfillProgress(job));
      await loadVersion(selectedReleaseId);
    } catch (cause) {
      setBackfillError(getErrorMessage(cause));
    } finally {
      setBackfilling(false);
    }
  }, [loadVersion, selectedReleaseId]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!loading) void loadVersion(selectedReleaseId);
  }, [loadVersion, loading, selectedReleaseId]);

  useEffect(() => {
    if (!backfillJob || !hasTauriRuntime()) return;
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    void listen<KnowledgeJobProgress>("knowledge-job-progress", (event) => {
      if (disposed || event.payload.jobKey !== backfillJob.jobKey) return;
      setBackfillProgress(event.payload);
      if (isTerminalJobStatus(event.payload.status)) {
        void loadVersion(selectedReleaseId);
      }
    })
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      })
      .catch((cause: unknown) => {
        if (!disposed) setBackfillError(getErrorMessage(cause));
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [backfillJob, loadVersion, selectedReleaseId]);

  useEffect(() => {
    if (!backfillJob) return;
    let disposed = false;
    let polling = false;
    let timer: number | undefined;
    const refreshJob = async () => {
      if (polling || disposed) return;
      polling = true;
      try {
        const job = await knowledgeJobsApi.get(backfillJob.jobKey);
        if (disposed) return;
        setBackfillProgress(toBackfillProgress(job));
        if (isTerminalJobStatus(job.status)) {
          void loadVersion(selectedReleaseId);
          if (timer != null) window.clearInterval(timer);
          return;
        }
      } catch (cause) {
        if (!disposed) setBackfillError(getErrorMessage(cause));
      } finally {
        polling = false;
      }
    };
    void refreshJob();
    timer = window.setInterval(() => void refreshJob(), 1000);
    return () => {
      disposed = true;
      if (timer != null) window.clearInterval(timer);
    };
  }, [backfillJob, loadVersion, selectedReleaseId]);

  if (loading) return <Skeleton active className="mt-8 w-full px-6" />;

  if (!project) {
    return (
      <main className="mt-8 w-full px-6">
        <Empty description={error || "没有找到这个项目"}>
          <Button onClick={() => navigate("/knowledge/projects")}>
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
      <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <Title level={2} className="!mb-1">
            项目版本
          </Title>
          <Paragraph type="secondary" className="!mb-0">
            选择一个版本即可查看多仓库 Commit 清单和知识处理进度。
          </Paragraph>
        </div>
        <Space wrap>
          <Button icon={<RefreshCw size={16} />} onClick={() => void load()}>
            刷新
          </Button>
          <Button
            icon={<ShieldCheck size={16} />}
            onClick={() =>
              navigate(`/knowledge/projects/${project.id}/sources`)
            }
          >
            管理来源授权
          </Button>
          {hasDocumentProcessingGaps && selectedReleaseId != null ? (
            <Popconfirm
              title="补齐历史文档处理"
              description="将重新解析当前版本缺少解析产物或全文索引的冻结正文；不会读取 Git 工作区，也不会修改原文或版本清单。"
              okText="开始回填"
              cancelText="取消"
              onConfirm={() => void startBackfill()}
            >
              <Button icon={<Wrench size={16} />} loading={backfilling}>
                补齐历史处理
              </Button>
            </Popconfirm>
          ) : null}
          <Button
            type="primary"
            icon={<Settings2 size={16} />}
            onClick={() => navigate(`/knowledge/projects/${project.id}/setup`)}
          >
            登记版本
          </Button>
        </Space>
      </div>

      {error ? (
        <Alert
          className="mb-4"
          type="error"
          showIcon
          title="无法读取版本信息"
          description={error}
          action={
            <Button onClick={() => void loadVersion(selectedReleaseId)}>
              重试
            </Button>
          }
        />
      ) : null}

      {backfillError ? (
        <Alert
          className="mb-4"
          type="error"
          showIcon
          title="历史处理回填未启动"
          description={backfillError}
        />
      ) : null}

      {backfillJob ? (
        <Alert
          className="mb-4"
          type={backfillProgress?.status === "failed" ? "error" : "info"}
          showIcon
          title={
            backfillProgress?.status === "completed"
              ? "历史处理回填已完成"
              : backfillProgress?.status === "cancelled"
                ? "历史处理回填已取消"
                : backfillProgress?.status === "failed"
                  ? "历史处理回填失败"
                  : "历史处理回填执行中"
          }
          description={
            <Space orientation="vertical" size={4} className="w-full">
              <Text>{backfillProgress?.message ?? backfillJob.message}</Text>
              <Progress
                percent={progressPercent(
                  backfillProgress?.current ?? backfillJob.progressCurrent,
                  backfillProgress?.total ?? backfillJob.progressTotal,
                )}
                size="small"
                showInfo
              />
              {backfillProgress?.error ? (
                <Text type="danger">{backfillProgress.error.message}</Text>
              ) : null}
            </Space>
          }
        />
      ) : null}

      {!releases.length ? (
        <Empty description="还没有登记项目版本">
          <Button
            type="primary"
            icon={<GitBranch size={16} />}
            onClick={() => navigate(`/knowledge/projects/${project.id}/setup`)}
          >
            开始登记版本
          </Button>
        </Empty>
      ) : (
        <>
          <Card className="mb-4" title="查看版本">
            <label className="block" htmlFor="knowledge-project-version">
              <Text strong>项目版本</Text>
              <Select
                id="knowledge-project-version"
                className="mt-2 w-full"
                aria-label="项目版本"
                value={selectedReleaseId}
                onChange={(value: number) => setSelectedReleaseId(value)}
                options={releases.map((release) => ({
                  value: release.id,
                  label: release.version,
                }))}
              />
            </label>
          </Card>

          {detailLoading ? <Skeleton active /> : null}
          {!detailLoading && manifest && completeness ? (
            <div className="grid gap-4 lg:grid-cols-2">
              <Card
                title="仓库清单"
                extra={
                  <Tag color={manifest.status === "ready" ? "green" : "gold"}>
                    {manifest.status === "ready" ? "清单已就绪" : "清单待处理"}
                  </Tag>
                }
              >
                <List
                  dataSource={manifest.repositories}
                  locale={{ emptyText: "该版本没有仓库清单" }}
                  renderItem={(item) => (
                    <List.Item>
                      <List.Item.Meta
                        title={
                          <Space wrap>
                            <Text strong>{item.requestedRefName}</Text>
                            <Tag>{refTypeLabel(item.requestedRefType)}</Tag>
                            {item.inclusionStatus === "excluded" ? (
                              <Tag>已排除</Tag>
                            ) : null}
                          </Space>
                        }
                        description={
                          <Space orientation="vertical" size={2}>
                            <Text type="secondary">
                              {item.inclusionStatus === "excluded"
                                ? item.exclusionReason || "已按规则排除"
                                : "Commit 已冻结，可安全追溯"}
                            </Text>
                            <Text
                              type="secondary"
                              className="font-mono text-xs"
                            >
                              {item.resolvedCommitSha || "等待解析 Commit"}
                            </Text>
                          </Space>
                        }
                      />
                    </List.Item>
                  )}
                />
              </Card>

              <Card
                title="知识处理进度"
                extra={
                  <Tag
                    color={completeness.status === "ready" ? "green" : "gold"}
                  >
                    {completeness.status === "ready"
                      ? "版本已就绪"
                      : "仍有待处理项"}
                  </Tag>
                }
              >
                <List
                  dataSource={completeness.stages}
                  renderItem={(stage) => (
                    <List.Item>
                      <div className="w-full">
                        <div className="mb-1 flex justify-between gap-3">
                          <Text strong>{stage.label}</Text>
                          <Tag color={stageColor(stage.status)}>
                            {stageStatusLabel(stage.status)}
                          </Tag>
                        </div>
                        <Progress
                          percent={progressPercent(
                            stage.completedCount,
                            stage.totalCount,
                          )}
                          size="small"
                          status={
                            stage.status === "partial" ? "active" : "normal"
                          }
                          showInfo={false}
                        />
                        <Text type="secondary" className="text-xs">
                          {stage.summary}
                        </Text>
                      </div>
                    </List.Item>
                  )}
                />
              </Card>
            </div>
          ) : null}
        </>
      )}
    </main>
  );
}

function refTypeLabel(type: string) {
  return { branch: "分支", tag: "Tag", commit: "Commit" }[type] ?? "引用";
}

function stageStatusLabel(status: string) {
  return (
    {
      ready: "已完成",
      partial: "进行中",
      pending: "待处理",
      not_started: "未开始",
    }[status] ?? "未知"
  );
}

function stageColor(status: string) {
  if (status === "ready") return "green";
  if (status === "partial") return "blue";
  return "default";
}

function progressPercent(completed: number, total: number) {
  if (total <= 0) return 0;
  return Math.min(100, Math.round((completed / total) * 100));
}
