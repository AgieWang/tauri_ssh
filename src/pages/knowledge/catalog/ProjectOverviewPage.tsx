import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Alert,
  Button,
  Card,
  Descriptions,
  Empty,
  Skeleton,
  Space,
  Tag,
  Typography,
} from "antd";
import {
  ArrowLeft,
  ScanSearch,
  FilePlus2,
  Files,
  GitBranch,
  Layers3,
  MessageCircleQuestion,
  Network,
  RefreshCw,
  Search,
  Settings2,
} from "lucide-react";
import { getErrorMessage } from "@/lib/api";
import { knowledgeCatalogApi } from "@/lib/api/knowledge-domain";
import type { KnowledgeProject } from "@/types";

const { Paragraph, Text, Title } = Typography;

/** 项目概览只呈现下一步，不把仓库、版本和索引参数堆到同一页面。 */
export default function ProjectOverviewPage() {
  const navigate = useNavigate();
  const { projectId } = useParams();
  const numericProjectId = Number(projectId);
  const [project, setProject] = useState<KnowledgeProject | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const projectRequestId = useRef(0);

  const loadProject = useCallback(async () => {
    const requestId = ++projectRequestId.current;
    if (!Number.isSafeInteger(numericProjectId) || numericProjectId < 1) {
      if (requestId === projectRequestId.current) {
        setError("项目地址无效");
        setLoading(false);
      }
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const result = await knowledgeCatalogApi.listProjects({
        projectId: numericProjectId,
        limit: 1,
        offset: 0,
      });
      if (requestId === projectRequestId.current) {
        setProject(result.items[0] ?? null);
      }
    } catch (nextError) {
      if (requestId === projectRequestId.current) {
        setError(getErrorMessage(nextError));
      }
    } finally {
      if (requestId === projectRequestId.current) {
        setLoading(false);
      }
    }
  }, [numericProjectId]);

  useEffect(() => {
    void loadProject();
  }, [loadProject]);

  if (loading) {
    return <Skeleton active className="mt-8 w-full px-6" />;
  }

  if (error) {
    return (
      <main className="mt-8 w-full px-6">
        <Alert
          type="error"
          showIcon
          title="无法打开项目"
          description={error}
          action={<Button onClick={() => void loadProject()}>重试</Button>}
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
        onClick={() => navigate("/knowledge/projects")}
      >
        返回项目列表
      </Button>
      <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <Space size={8} wrap>
            <Title level={2} className="!mb-0">
              {project.name}
            </Title>
            {project.enabled ? (
              <Tag color="green">使用中</Tag>
            ) : (
              <Tag>已暂停</Tag>
            )}
          </Space>
          <Paragraph type="secondary" className="!mb-0 !mt-2">
            {project.description || "尚未添加项目说明"}
          </Paragraph>
        </div>
        <Button
          icon={<RefreshCw size={16} />}
          onClick={() => void loadProject()}
        >
          刷新
        </Button>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <Card title="项目准备情况">
          <Descriptions column={1} size="small">
            <Descriptions.Item label="代码仓库">
              {project.gitWorkspaceKeys.length} 个
            </Descriptions.Item>
            <Descriptions.Item label="当前状态">
              {project.enabled ? "可以继续使用" : "已暂停"}
            </Descriptions.Item>
          </Descriptions>
        </Card>
        <Card title="下一步">
          <Space orientation="vertical" size={10} className="w-full">
            <Text>
              {project.gitWorkspaceKeys.length
                ? "项目已关联代码仓库，可继续在项目工作台中处理文档。"
                : "该项目尚未关联代码仓库。现在可继续完成仓库和初始版本设置。"}
            </Text>
            {project.gitWorkspaceKeys.length === 0 ? (
              <Button
                type="primary"
                icon={<GitBranch size={16} />}
                onClick={() =>
                  navigate(`/knowledge/projects/${project.id}/setup`)
                }
              >
                继续设置
              </Button>
            ) : null}
          </Space>
        </Card>
        <Card title="项目文档">
          <Space orientation="vertical" size={10} className="w-full">
            <Text>
              查看、搜索和维护项目资料；新建与上传入口保留在同一项目范围内。
            </Text>
            <Space wrap>
              <Button
                icon={<Files size={16} />}
                onClick={() =>
                  navigate(`/knowledge/projects/${project.id}/documents`)
                }
              >
                查看文档
              </Button>
              <Button
                icon={<FilePlus2 size={16} />}
                onClick={() =>
                  navigate(`/knowledge/projects/${project.id}/documents/new`)
                }
              >
                添加文档
              </Button>
            </Space>
          </Space>
        </Card>
        <Card title="项目版本">
          <Space orientation="vertical" size={10} className="w-full">
            <Text>
              查看多仓库 Commit 清单，以及文档、索引、分析和向量处理进度。
            </Text>
            <Button
              icon={<Layers3 size={16} />}
              onClick={() =>
                navigate(`/knowledge/projects/${project.id}/versions`)
              }
            >
              管理项目版本
            </Button>
          </Space>
        </Card>
        <Card title="向量化与索引">
          <Space orientation="vertical" size={10} className="w-full">
            <Text>
              配置当前设备的全局本地索引方案；构建或重建不会只处理当前项目。
            </Text>
            <Text type="secondary">
              项目问答与检索仍会按所选项目和版本过滤。
            </Text>
            <Button
              icon={<Settings2 size={16} />}
              onClick={() =>
                navigate(`/knowledge/projects/${project.id}/embedding`)
              }
            >
              配置向量化与索引
            </Button>
          </Space>
        </Card>
        <Card title="搜索项目知识">
          <Space orientation="vertical" size={10} className="w-full">
            <Text>在当前项目和指定版本内搜索标题与文档正文。</Text>
            <Button
              icon={<Search size={16} />}
              onClick={() =>
                navigate(`/knowledge/projects/${project.id}/search`)
              }
            >
              搜索项目知识
            </Button>
          </Space>
        </Card>
        <Card title="源码分析">
          <Space orientation="vertical" size={10} className="w-full">
            <Text>
              选择仓库与版本，捕获只读 Commit 后生成静态项目分析报告。
            </Text>
            <Button
              icon={<ScanSearch size={16} />}
              disabled={project.gitWorkspaceKeys.length === 0}
              onClick={() =>
                navigate(`/knowledge/projects/${project.id}/analysis`)
              }
            >
              分析项目代码
            </Button>
          </Space>
        </Card>
        <Card title="项目知识图谱">
          <Space orientation="vertical" size={10} className="w-full">
            <Text>按项目版本将文档与已确认关系整理为可追溯的关系视图。</Text>
            <Button
              icon={<Network size={16} />}
              onClick={() =>
                navigate(`/knowledge/projects/${project.id}/graph`)
              }
            >
              查看知识图谱
            </Button>
          </Space>
        </Card>
        <Card title="项目知识问答">
          <Space orientation="vertical" size={10} className="w-full">
            <Text>
              选择项目版本后，基于已引用的文档、向量和关系证据回答问题。
            </Text>
            <Button
              icon={<MessageCircleQuestion size={16} />}
              onClick={() => navigate(`/knowledge/projects/${project.id}/qa`)}
            >
              提问项目知识
            </Button>
          </Space>
        </Card>
      </div>

      {project.gitWorkspaceKeys.length === 0 ? (
        <Alert
          className="mt-4"
          type="info"
          showIcon
          title="还没有代码仓库"
          description="继续设置后，系统会帮助你选择代码仓库和初始版本。"
          action={
            <Button
              type="primary"
              icon={<GitBranch size={16} />}
              onClick={() =>
                navigate(`/knowledge/projects/${project.id}/setup`)
              }
            >
              继续设置
            </Button>
          }
        />
      ) : null}
    </main>
  );
}
