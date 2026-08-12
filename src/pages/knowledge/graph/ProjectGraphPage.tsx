import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Alert,
  Button,
  Card,
  Empty,
  Result,
  Select,
  Skeleton,
  Space,
  Table,
  Tag,
  Tooltip,
  Typography,
  message,
} from "antd";
import { ArrowLeft, Network, RefreshCw, Sparkles } from "lucide-react";
import { getErrorMessage } from "@/lib/api";
import {
  knowledgeCatalogApi,
  knowledgeGraphApi,
} from "@/lib/api/knowledge-domain";
import type {
  KnowledgeGraphProjection,
  KnowledgeProject,
  KnowledgeRelease,
} from "@/types";

const { Paragraph, Text, Title } = Typography;

const entityTypeLabels: Record<string, string> = {
  api: "接口",
  bug: "缺陷",
  code_file: "代码文件",
  code_snapshot: "代码快照",
  code_symbol: "代码元素",
  commit: "提交记录",
  document: "文档",
  git_branch: "代码分支",
  git_commit: "代码版本",
  git_tag: "代码标签",
  requirement: "需求",
  release: "项目版本",
  task: "任务",
  test: "测试",
};

function entityTypeLabel(entityType: string) {
  return entityTypeLabels[entityType] ?? "其他信息";
}

const relationTypeLabels: Record<string, string> = {
  calls: "调用",
  contains: "包含",
  depends_on: "依赖",
  describes: "说明",
  implements: "实现",
  imports: "引用",
  references: "引用",
  related_to: "关联",
  tested_by: "由其测试",
  uses: "使用",
};

function relationTypeLabel(relationType: string) {
  return relationTypeLabels[relationType] ?? `关联（${relationType}）`;
}

function displayEntityLabel({
  entityKey,
  entityType,
  label,
}: KnowledgeGraphProjection["nodes"][number]) {
  const rawLabel = (label || entityKey).replace(
    new RegExp(`^${entityType}\\s*:\\s*`, "i"),
    "",
  );
  if (entityType === "git_commit") return `提交 ${rawLabel.slice(0, 12)}`;
  if (entityType === "code_snapshot") return `快照 #${rawLabel}`;
  if (entityType === "release") return `版本 ${rawLabel}`;
  return rawLabel.length > 28 ? `${rawLabel.slice(0, 27)}…` : rawLabel;
}

function codeElementPath({
  entityKey,
  entityType,
  label,
}: KnowledgeGraphProjection["nodes"][number]) {
  return (label || entityKey).replace(
    new RegExp(`^${entityType}\\s*:\\s*`, "i"),
    "",
  );
}

function codeElementFileName(node: KnowledgeGraphProjection["nodes"][number]) {
  const path = codeElementPath(node).replace(/\\/g, "/");
  const filename = path.slice(path.lastIndexOf("/") + 1);
  return filename.split(/[#:]/, 1)[0] || filename;
}

function entityDisplayLabel(node: KnowledgeGraphProjection["nodes"][number]) {
  return node.entityType === "code_symbol"
    ? codeElementFileName(node)
    : displayEntityLabel(node);
}

function entitySummary(node: KnowledgeGraphProjection["nodes"][number]) {
  return `${entityTypeLabel(node.entityType)} · ${entityDisplayLabel(node)}`;
}

function EntitySummary({
  node,
}: {
  node: KnowledgeGraphProjection["nodes"][number];
}) {
  const summary = entitySummary(node);
  if (node.entityType !== "code_symbol") return summary;
  const fullPath = codeElementPath(node);
  return (
    <Tooltip title={fullPath}>
      <span
        aria-label={`代码元素完整路径：${fullPath}`}
        className="cursor-help"
        tabIndex={0}
      >
        {summary}
      </span>
    </Tooltip>
  );
}

const MAX_VISIBLE_GRAPH_NODES = 15;

/** 默认只绘制可读的关键子图；完整关系仍保留在下方表格以便核查。 */
function selectVisibleProjection(projection: KnowledgeGraphProjection) {
  if (projection.nodes.length <= MAX_VISIBLE_GRAPH_NODES) return projection;

  const degree = new Map<number, number>();
  projection.edges.forEach((edge) => {
    degree.set(edge.fromNodeId, (degree.get(edge.fromNodeId) ?? 0) + 1);
    degree.set(edge.toNodeId, (degree.get(edge.toNodeId) ?? 0) + 1);
  });
  const selectedNodes = [...projection.nodes]
    .sort(
      (left, right) => (degree.get(right.id) ?? 0) - (degree.get(left.id) ?? 0),
    )
    .slice(0, MAX_VISIBLE_GRAPH_NODES);
  const selectedIds = new Set(selectedNodes.map((node) => node.id));
  return {
    ...projection,
    nodes: selectedNodes,
    edges: projection.edges
      .filter(
        (edge) =>
          selectedIds.has(edge.fromNodeId) && selectedIds.has(edge.toNodeId),
      )
      .slice(0, 20),
  };
}

/** 使用分层布局展示小型关键子图，避免大量实体挤在同一个圆环中。 */
function GraphCanvas({ projection }: { projection: KnowledgeGraphProjection }) {
  const visible = useMemo(
    () => selectVisibleProjection(projection),
    [projection],
  );
  const layout = useMemo(() => {
    const incoming = new Set(visible.edges.map((edge) => edge.toNodeId));
    const sources = visible.nodes.filter((node) => !incoming.has(node.id));
    const leftColumn = sources.length ? sources : visible.nodes.slice(0, 1);
    const leftIds = new Set(leftColumn.map((node) => node.id));
    const rightColumn = visible.nodes.filter((node) => !leftIds.has(node.id));
    const rows = Math.max(leftColumn.length, rightColumn.length, 1);
    const height = Math.max(300, rows * 84 + 72);
    const points = new Map<number, { x: number; y: number }>();
    leftColumn.forEach((node, index) =>
      points.set(node.id, {
        x: 165,
        y: 56 + (index + 0.5) * ((height - 112) / leftColumn.length),
      }),
    );
    rightColumn.forEach((node, index) =>
      points.set(node.id, {
        x: 805,
        y: 56 + (index + 0.5) * ((height - 112) / rightColumn.length),
      }),
    );
    return { height, points };
  }, [visible]);

  return (
    <div className="overflow-x-auto rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] p-3">
      {visible.nodes.length < projection.nodes.length ? (
        <Alert
          className="mb-3"
          showIcon
          type="info"
          title={`为保证可读性，图中仅展示 ${visible.nodes.length} 个关键实体和 ${visible.edges.length} 条关系；完整 ${projection.edges.length} 条关系请在下方清单查看。`}
        />
      ) : null}
      <svg
        aria-label="项目知识图谱关系视图：箭头从起点指向终点"
        className="block min-w-[970px]"
        height={layout.height}
        role="img"
        viewBox={`0 0 970 ${layout.height}`}
        width="970"
      >
        <title>项目知识图谱关系视图，箭头从起点指向终点</title>
        <defs>
          <marker
            id="knowledge-graph-arrow"
            markerHeight="8"
            markerWidth="8"
            orient="auto"
            refX="7"
            refY="3"
          >
            <path d="M0,0 L0,6 L7,3 z" fill="var(--text-secondary)" />
          </marker>
        </defs>
        {visible.edges.map((edge) => {
          const from = layout.points.get(edge.fromNodeId);
          const to = layout.points.get(edge.toNodeId);
          if (!from || !to) return null;
          const relationX = (from.x + to.x) / 2;
          const relationY = (from.y + to.y) / 2;
          return (
            <g key={edge.id}>
              <line
                stroke="var(--text-secondary)"
                strokeDasharray={edge.confirmed ? undefined : "5 4"}
                strokeWidth="1.5"
                x1={from.x + 110}
                x2={to.x - 110}
                y1={from.y}
                y2={to.y}
                markerEnd="url(#knowledge-graph-arrow)"
              />
              <rect
                fill="var(--bg-secondary)"
                height="22"
                rx="5"
                width="112"
                x={relationX - 56}
                y={relationY - 11}
              />
              <text
                fill="var(--text-secondary)"
                fontSize="12"
                textAnchor="middle"
                x={relationX}
                y={relationY + 4}
              >
                {relationTypeLabel(edge.relationType)}
              </text>
            </g>
          );
        })}
        {visible.nodes.map((node) => {
          const point = layout.points.get(node.id);
          if (!point) return null;
          return (
            <g key={node.id}>
              {node.entityType === "code_symbol" ? (
                <title>{codeElementPath(node)}</title>
              ) : null}
              <rect
                fill="var(--primary-color)"
                height="58"
                rx="10"
                width="220"
                x={point.x - 110}
                y={point.y - 29}
              />
              <text
                fill="white"
                fontSize="12"
                textAnchor="middle"
                x={point.x}
                y={point.y - 5}
              >
                {entityTypeLabel(node.entityType)}
              </text>
              <text
                fill="white"
                fontSize="11"
                textAnchor="middle"
                x={point.x}
                y={point.y + 15}
              >
                {entityDisplayLabel(node)}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

/** 图谱工作台把选择版本、生成和查看拆成线性动作，避免普通用户面对关系配置细节。 */
export default function ProjectGraphPage() {
  const navigate = useNavigate();
  const { projectId } = useParams();
  const numericProjectId = Number(projectId);
  const [project, setProject] = useState<KnowledgeProject | null>(null);
  const [releases, setReleases] = useState<KnowledgeRelease[]>([]);
  const [releaseId, setReleaseId] = useState<number | null>(null);
  const [projection, setProjection] = useState<KnowledgeGraphProjection | null>(
    null,
  );
  const [rootNodeId, setRootNodeId] = useState<number | null>(null);
  const [depth, setDepth] = useState(2);
  const [includeUnconfirmed, setIncludeUnconfirmed] = useState(false);
  const [loading, setLoading] = useState(true);
  const [building, setBuilding] = useState(false);
  const [querying, setQuerying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestId = useRef(0);
  const operationId = useRef(0);
  const operationRunning = useRef(false);

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
      const projects = await knowledgeCatalogApi.listProjects({
        projectId: numericProjectId,
        limit: 1,
        offset: 0,
      });
      const selectedProject =
        projects.items.find((item) => item.id === numericProjectId) ?? null;
      const projectReleases = selectedProject
        ? await knowledgeCatalogApi.listReleases(selectedProject.id)
        : [];
      if (currentRequestId !== requestId.current) return;
      setProject(selectedProject);
      setReleases(projectReleases);
      setReleaseId((current) =>
        current != null &&
        projectReleases.some((release) => release.id === current)
          ? current
          : (projectReleases[0]?.id ?? null),
      );
    } catch (cause) {
      if (currentRequestId === requestId.current)
        setError(getErrorMessage(cause));
    } finally {
      if (currentRequestId === requestId.current) setLoading(false);
    }
  }, [numericProjectId]);

  const query = useCallback(
    async (nextRootNodeId: number | null = rootNodeId) => {
      if (!project || releaseId == null || operationRunning.current) return;
      const rootNode =
        projection?.nodes.find((node) => node.id === nextRootNodeId) ?? null;
      const currentRequestId = requestId.current;
      const currentOperationId = ++operationId.current;
      operationRunning.current = true;
      setQuerying(true);
      setError(null);
      try {
        const result = await knowledgeGraphApi.queryProjectGraph({
          projectId: project.id,
          projectVersionId: releaseId,
          rootEntityKey: rootNode?.entityKey ?? null,
          rootEntityType: rootNode?.entityType ?? null,
          depth,
          nodeLimit: 80,
          includeUnconfirmed,
        });
        if (
          currentRequestId !== requestId.current ||
          currentOperationId !== operationId.current
        )
          return;
        setProjection(result);
        setRootNodeId(nextRootNodeId);
      } catch (cause) {
        if (
          currentRequestId === requestId.current &&
          currentOperationId === operationId.current
        )
          setError(getErrorMessage(cause));
      } finally {
        if (currentOperationId === operationId.current) {
          operationRunning.current = false;
          setQuerying(false);
        }
      }
    },
    [depth, includeUnconfirmed, project, projection, releaseId, rootNodeId],
  );

  useEffect(() => {
    setProject(null);
    setReleases([]);
    setReleaseId(null);
    setProjection(null);
    setRootNodeId(null);
    void load();
    return () => {
      requestId.current += 1;
      operationId.current += 1;
      operationRunning.current = false;
      setBuilding(false);
      setQuerying(false);
    };
  }, [load]);

  async function build() {
    if (!project || releaseId == null || operationRunning.current) return;
    const currentRequestId = requestId.current;
    const currentOperationId = ++operationId.current;
    operationRunning.current = true;
    setBuilding(true);
    setError(null);
    try {
      const result = await knowledgeGraphApi.buildProjectGraph({
        projectId: project.id,
        projectVersionId: releaseId,
        includeUnconfirmed,
      });
      if (
        currentRequestId !== requestId.current ||
        currentOperationId !== operationId.current
      )
        return;
      message.success(
        result.reused
          ? `已使用当前图谱（${result.nodeCount} 个实体，${result.edgeCount} 条关系）`
          : `图谱已生成：${result.nodeCount} 个实体，${result.edgeCount} 条关系`,
      );
      setRootNodeId(null);
      const graph = await knowledgeGraphApi.queryProjectGraph({
        projectId: project.id,
        projectVersionId: releaseId,
        rootEntityKey: null,
        rootEntityType: null,
        depth,
        nodeLimit: 80,
        includeUnconfirmed,
      });
      if (
        currentRequestId !== requestId.current ||
        currentOperationId !== operationId.current
      )
        return;
      setProjection(graph);
    } catch (cause) {
      if (
        currentRequestId === requestId.current &&
        currentOperationId === operationId.current
      )
        setError(getErrorMessage(cause));
    } finally {
      if (currentOperationId === operationId.current) {
        operationRunning.current = false;
        setBuilding(false);
      }
    }
  }

  if (loading) return <Skeleton active className="mt-8 w-full px-6" />;

  if (!project) {
    return (
      <main className="mt-8 w-full px-6">
        <Result
          status="warning"
          title="无法打开知识图谱"
          subTitle={error || "没有找到这个项目"}
          extra={
            <Button onClick={() => navigate("/knowledge/projects")}>
              返回项目列表
            </Button>
          }
        />
      </main>
    );
  }

  const busy = building || querying;
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
            项目知识图谱
          </Title>
          <Paragraph type="secondary" className="!mb-0">
            选择项目版本后，系统在本地根据文档和已确认关系生成可追溯的知识图谱。
          </Paragraph>
        </div>
        <Button
          icon={<RefreshCw size={16} />}
          disabled={busy}
          onClick={() => void load()}
        >
          刷新
        </Button>
      </div>

      <Alert
        className="mb-4"
        type="info"
        showIcon
        title="图谱不会把文档发送到远程服务"
        description="默认仅使用已确认关系；每条边会保留来源关系和文档版本证据。重新生成失败时，当前已启用图谱不会被替换。"
      />
      {error ? (
        <Alert
          className="mb-4"
          type="error"
          showIcon
          title="操作未完成"
          description={error}
        />
      ) : null}

      {!releases.length ? (
        <Empty description="请先创建一个项目版本">
          <Button
            type="primary"
            onClick={() =>
              navigate(`/knowledge/projects/${project.id}/versions`)
            }
          >
            管理项目版本
          </Button>
        </Empty>
      ) : (
        <>
          <Card className="mb-4" title="1. 选择版本并生成">
            <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_auto]">
              <label className="block" htmlFor="knowledge-graph-version">
                <Text strong>项目版本</Text>
                <Select
                  id="knowledge-graph-version"
                  aria-label="项目版本"
                  className="mt-2 w-full"
                  value={releaseId}
                  disabled={busy}
                  onChange={(value: number) => {
                    setReleaseId(value);
                    setProjection(null);
                    setRootNodeId(null);
                  }}
                  options={releases.map((release) => ({
                    value: release.id,
                    label: release.version,
                  }))}
                />
              </label>
              <label
                className="block md:self-end"
                htmlFor="knowledge-graph-candidates"
              >
                <Text strong>显示范围</Text>
                <Select
                  id="knowledge-graph-candidates"
                  aria-label="显示范围"
                  className="mt-2 w-48"
                  value={includeUnconfirmed ? "all" : "confirmed"}
                  disabled={busy}
                  onChange={(value: string) =>
                    setIncludeUnconfirmed(value === "all")
                  }
                  options={[
                    { value: "confirmed", label: "仅已确认关系" },
                    { value: "all", label: "含待确认候选" },
                  ]}
                />
              </label>
            </div>
            <Button
              className="mt-4"
              type="primary"
              icon={<Sparkles size={16} />}
              loading={building}
              disabled={busy || releaseId == null}
              onClick={() => void build()}
            >
              生成知识图谱
            </Button>
          </Card>

          <Card className="mb-4" title="2. 查看关系">
            <div className="grid gap-4 md:grid-cols-2">
              <label className="block" htmlFor="knowledge-graph-root">
                <Text strong>从指定实体查看（可选）</Text>
                <Select
                  id="knowledge-graph-root"
                  aria-label="从指定实体查看"
                  className="mt-2 w-full"
                  allowClear
                  disabled={busy || !projection}
                  placeholder="显示当前版本图谱总览"
                  value={rootNodeId}
                  onChange={(value: number | undefined) => {
                    const nextRootNodeId = value ?? null;
                    void query(nextRootNodeId);
                  }}
                  options={projection?.nodes.map((node) => ({
                    value: node.id,
                    label: entitySummary(node),
                  }))}
                />
              </label>
              <label className="block" htmlFor="knowledge-graph-depth">
                <Text strong>关联层级</Text>
                <Select
                  id="knowledge-graph-depth"
                  aria-label="关联层级"
                  className="mt-2 w-full"
                  value={depth}
                  disabled={busy || !projection}
                  onChange={(value: number) => setDepth(value)}
                  options={[1, 2, 3, 4].map((value) => ({
                    value,
                    label: `${value} 层关联`,
                  }))}
                />
              </label>
            </div>
            <Button
              className="mt-4"
              icon={<Network size={16} />}
              loading={querying}
              disabled={busy || !projection || releaseId == null}
              onClick={() => void query()}
            >
              更新视图
            </Button>
          </Card>

          {projection ? (
            <>
              <Card className="mb-4" title="关系视图">
                <Space className="mb-3" wrap>
                  <Tag color="blue">{projection.nodes.length} 个实体</Tag>
                  <Tag color="purple">{projection.edges.length} 条关系</Tag>
                  {projection.truncated ? (
                    <Tag color="gold">结果已按上限截断</Tag>
                  ) : null}
                </Space>
                <Alert
                  className="mb-3"
                  showIcon
                  type="info"
                  message="阅读方法：从左侧或上游的起点，沿箭头经过关系名称，到达终点。"
                  description="节点第一行是实体类型，第二行是业务名称；实线表示已确认关系，虚线表示待确认候选。每一条关系均可在下方清单查看其证据。"
                />
                <GraphCanvas projection={projection} />
              </Card>
              <Card title="关系清单（起点 → 关系 → 终点）">
                <Table
                  rowKey="id"
                  size="small"
                  pagination={{ pageSize: 10, showSizeChanger: false }}
                  dataSource={projection.edges}
                  columns={[
                    {
                      title: "起点",
                      render: (_, edge) =>
                        (() => {
                          const node = projection.nodes.find(
                            (item) => item.id === edge.fromNodeId,
                          );
                          return node ? (
                            <EntitySummary node={node} />
                          ) : (
                            "未知起点"
                          );
                        })(),
                    },
                    {
                      title: "关系",
                      render: (_, edge) => relationTypeLabel(edge.relationType),
                    },
                    {
                      title: "终点",
                      render: (_, edge) =>
                        (() => {
                          const node = projection.nodes.find(
                            (item) => item.id === edge.toNodeId,
                          );
                          return node ? (
                            <EntitySummary node={node} />
                          ) : (
                            "未知终点"
                          );
                        })(),
                    },
                    {
                      title: "证据",
                      render: (_, edge) => {
                        const documentVersionId =
                          edge.evidence.documentVersionId;
                        return documentVersionId
                          ? `文档版本 #${String(documentVersionId)}`
                          : edge.sourceRelationRef;
                      },
                    },
                    {
                      title: "状态",
                      render: (_, edge) =>
                        edge.confirmed ? (
                          <Tag color="green">已确认</Tag>
                        ) : (
                          <Tag color="gold">待确认</Tag>
                        ),
                    },
                  ]}
                />
              </Card>
            </>
          ) : (
            <Empty description="选择版本后生成知识图谱，即可在这里查看关系" />
          )}
        </>
      )}
    </main>
  );
}
