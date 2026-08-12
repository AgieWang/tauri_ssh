import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Alert,
  Button,
  Card,
  Empty,
  List,
  Progress,
  Select,
  Skeleton,
  Space,
  Tag,
  Typography,
} from "antd";
import { ArrowLeft, GitBranch, RefreshCw, Settings2 } from "lucide-react";
import { getErrorMessage } from "@/lib/api";
import { knowledgeCatalogApi } from "@/lib/api/knowledge-domain";
import type { KnowledgeProject, KnowledgeRelease } from "@/types";
import type {
  KnowledgeProjectVersionCompleteness,
  KnowledgeProjectVersionManifestResult,
} from "@/types/knowledge-domain/catalog";

const { Paragraph, Text, Title } = Typography;

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
  const [error, setError] = useState<string | null>(null);
  const requestId = useRef(0);

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

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!loading) void loadVersion(selectedReleaseId);
  }, [loadVersion, loading, selectedReleaseId]);

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
