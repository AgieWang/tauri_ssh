import { useCallback, useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Alert,
  Button,
  Card,
  Descriptions,
  Empty,
  Popconfirm,
  Select,
  Skeleton,
  Space,
  Table,
  Tag,
  Typography,
  message,
} from "antd";
import type { TableColumnsType } from "antd";
import { ArrowLeft, RefreshCw, ShieldCheck, Upload } from "lucide-react";
import { getErrorMessage, knowledgeApi } from "@/lib/api";
import { knowledgeCatalogApi } from "@/lib/api/knowledge-domain";
import type {
  KnowledgeProject,
  KnowledgeRelease,
  KnowledgeSource,
  UpsertKnowledgeSourceInput,
} from "@/types";

const { Paragraph, Text, Title } = Typography;

function sourceInput(
  source: KnowledgeSource,
  allowRemoteEmbedding: boolean,
): UpsertKnowledgeSourceInput {
  return {
    id: source.id,
    sourceKey: source.sourceKey,
    projectId: source.projectId,
    sourceType: source.sourceType,
    displayName: source.displayName,
    rootPath: source.rootPath,
    gitWorkspaceKey: source.gitWorkspaceKey,
    includeGlobs: source.includeGlobs,
    excludeGlobs: source.excludeGlobs,
    versionStrategy: source.versionStrategy,
    syncMode: source.syncMode,
    allowRemoteEmbedding,
    enabled: source.enabled,
  };
}

function syncStatus(source: KnowledgeSource) {
  if (source.lastSyncStatus === "success")
    return <Tag color="green">同步成功</Tag>;
  if (source.lastSyncStatus === "running")
    return <Tag color="blue">同步中</Tag>;
  if (source.lastSyncStatus === "failed")
    return <Tag color="red">同步失败</Tag>;
  return <Tag>{source.lastSyncStatus || "未同步"}</Tag>;
}

/** 当前项目的知识来源、同步与远程向量化授权均在此页处理，不能退化为全局来源列表。 */
export default function ProjectSourcesPage() {
  const navigate = useNavigate();
  const { projectId } = useParams();
  const numericProjectId = Number(projectId);
  const [project, setProject] = useState<KnowledgeProject | null>(null);
  const [sources, setSources] = useState<KnowledgeSource[]>([]);
  const [releases, setReleases] = useState<KnowledgeRelease[]>([]);
  const [releaseId, setReleaseId] = useState<number | undefined>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [updatingSourceId, setUpdatingSourceId] = useState<number | null>(null);
  const [syncingSourceId, setSyncingSourceId] = useState<number | null>(null);

  const load = useCallback(async () => {
    if (!Number.isSafeInteger(numericProjectId) || numericProjectId < 1) {
      setError("项目地址无效");
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const [projectPage, nextSources, nextReleases] = await Promise.all([
        knowledgeApi.listProjects({
          projectId: numericProjectId,
          limit: 1,
          offset: 0,
        }),
        knowledgeApi.listSources(numericProjectId),
        knowledgeApi.listReleases(numericProjectId),
      ]);
      setProject(projectPage.items[0] ?? null);
      setSources(nextSources);
      setReleases(nextReleases);
      setReleaseId((current) =>
        current != null &&
        nextReleases.some((release) => release.id === current)
          ? current
          : undefined,
      );
    } catch (cause) {
      setError(getErrorMessage(cause));
    } finally {
      setLoading(false);
    }
  }, [numericProjectId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function updateAuthorization(
    source: KnowledgeSource,
    nextValue: boolean,
  ) {
    setUpdatingSourceId(source.id);
    try {
      const updated = await knowledgeApi.upsertSource(
        sourceInput(source, nextValue),
      );
      setSources((current) =>
        current.map((item) => (item.id === updated.id ? updated : item)),
      );
      message.success(nextValue ? "已授权远程向量化" : "已撤销远程向量化授权");
    } catch (cause) {
      message.error(getErrorMessage(cause));
    } finally {
      setUpdatingSourceId(null);
    }
  }

  async function startSync(source: KnowledgeSource) {
    if (releaseId == null) {
      message.warning("请先选择要同步到的项目版本");
      return;
    }
    setSyncingSourceId(source.id);
    try {
      let gitRef: string | undefined;
      if (source.sourceType === "git_workspace") {
        const [bindings, manifest] = await Promise.all([
          knowledgeCatalogApi.listRepositoryBindings(numericProjectId),
          knowledgeCatalogApi.getProjectVersionManifest(releaseId),
        ]);
        const binding = bindings.find(
          (item) => item.workspaceKey === source.gitWorkspaceKey,
        );
        const repository = binding
          ? manifest.repositories.find(
              (item) => item.repositoryBindingId === binding.id,
            )
          : undefined;
        if (
          !repository ||
          repository.inclusionStatus !== "ready" ||
          !repository.resolvedCommitSha
        ) {
          throw new Error("当前版本清单中没有可同步的仓库 Commit");
        }
        gitRef = repository.resolvedCommitSha;
      }
      const job = await knowledgeApi.startSourceSync({
        sourceId: source.id,
        releaseId,
        gitRef,
      });
      message.success(`已启动同步任务：${job.jobKey}`);
      await load();
    } catch (cause) {
      message.error(getErrorMessage(cause));
    } finally {
      setSyncingSourceId(null);
    }
  }

  const columns: TableColumnsType<KnowledgeSource> = [
    {
      title: "仓库来源",
      dataIndex: "displayName",
      render: (value, row) => (
        <Space direction="vertical" size={0}>
          <Text strong>{value}</Text>
          <Text type="secondary" className="text-xs">
            {row.gitWorkspaceKey || row.sourceKey}
          </Text>
        </Space>
      ),
    },
    {
      title: "最近同步",
      render: (_, row) => (
        <Space direction="vertical" size={0}>
          {syncStatus(row)}
          <Text type="secondary" className="text-xs">
            {row.lastSyncedAt ?? "-"}
          </Text>
        </Space>
      ),
    },
    {
      title: "远程向量化",
      render: (_, row) =>
        row.allowRemoteEmbedding ? (
          <Tag color="green">已授权</Tag>
        ) : (
          <Tag>未授权</Tag>
        ),
    },
    {
      title: "操作",
      width: 280,
      render: (_, row) => (
        <Space wrap size="small">
          <Button
            type="link"
            icon={<Upload size={14} />}
            loading={syncingSourceId === row.id}
            onClick={() => void startSync(row)}
          >
            同步
          </Button>
          <Popconfirm
            title={
              row.allowRemoteEmbedding ? "撤销远程向量化授权" : "授权远程向量化"
            }
            description={
              row.allowRemoteEmbedding
                ? "撤销后，该仓库正文不会再发送给远程向量服务。"
                : "授权后，满足安全策略的该仓库文档正文可发送给当前远程向量服务构建向量。"
            }
            okText={row.allowRemoteEmbedding ? "撤销授权" : "确认授权"}
            cancelText="取消"
            onConfirm={() =>
              void updateAuthorization(row, !row.allowRemoteEmbedding)
            }
          >
            <Button
              type="link"
              icon={<ShieldCheck size={14} />}
              loading={updatingSourceId === row.id}
            >
              {row.allowRemoteEmbedding ? "撤销授权" : "授权"}
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  if (loading) return <Skeleton active className="mt-8 w-full px-6" />;

  if (error || !project) {
    return (
      <main className="mt-8 w-full px-6">
        <Empty description={error || "没有找到项目"}>
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
        onClick={() => navigate(`/knowledge/projects/${project.id}/versions`)}
      >
        返回项目版本
      </Button>
      <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <Title level={2} className="!mb-1">
            来源与同步
          </Title>
          <Paragraph type="secondary" className="!mb-0">
            仅管理“{project.name}”的知识来源、版本同步与远程向量化授权。
          </Paragraph>
        </div>
        <Button icon={<RefreshCw size={16} />} onClick={() => void load()}>
          刷新
        </Button>
      </div>

      <Alert
        className="mb-4"
        type="warning"
        showIcon
        title="远程向量化授权范围"
        description="授权只作用于当前行仓库。构建时仍会执行来源启用、敏感级别和内容安全检查；未授权来源的正文不会发送到远程服务。"
      />

      <Card className="mb-4" title="同步目标版本">
        <Descriptions column={{ xs: 1, sm: 2 }} size="small">
          <Descriptions.Item label="当前项目">{project.name}</Descriptions.Item>
          <Descriptions.Item label="同步到版本">
            <Select
              aria-label="同步到版本"
              className="min-w-56"
              value={releaseId}
              onChange={(value: number) => setReleaseId(value)}
              placeholder="选择项目版本"
              options={releases.map((release) => ({
                value: release.id,
                label: release.version,
              }))}
            />
          </Descriptions.Item>
        </Descriptions>
      </Card>

      <Card title="项目来源">
        <Table
          rowKey="id"
          columns={columns}
          dataSource={sources}
          pagination={false}
          locale={{ emptyText: "当前项目尚未登记知识来源" }}
        />
      </Card>
    </main>
  );
}
