import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  Alert,
  Button,
  Card,
  Drawer,
  Empty,
  Form,
  Input,
  Modal,
  Pagination,
  Space,
  Tag,
  Typography,
  message,
} from "antd";
import {
  ArrowRight,
  FolderPlus,
  MessageCircleQuestion,
  MoreHorizontal,
  Pencil,
  Power,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { getErrorMessage } from "@/lib/api";
import { knowledgeCatalogApi } from "@/lib/api/knowledge-domain";
import type { KnowledgeProject } from "@/types";
import { projectInput } from "./utils";

const { Paragraph, Text, Title } = Typography;
const PROJECT_PAGE_SIZE = 20;

interface ProjectEditValues {
  name: string;
  description: string;
}

/** 新知识工作台的项目入口；不复用旧知识页面的布局或交互状态。 */
export default function ProjectCatalogPage() {
  const navigate = useNavigate();
  const [projects, setProjects] = useState<KnowledgeProject[]>([]);
  const [projectPage, setProjectPage] = useState(1);
  const [projectTotal, setProjectTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [editing, setEditing] = useState<KnowledgeProject | null>(null);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm<ProjectEditValues>();
  const projectRequestId = useRef(0);

  const loadProjects = useCallback(async () => {
    const requestId = ++projectRequestId.current;
    setLoading(true);
    setLoadError(null);
    try {
      const result = await knowledgeCatalogApi.listProjects({
        limit: PROJECT_PAGE_SIZE,
        offset: (projectPage - 1) * PROJECT_PAGE_SIZE,
      });
      if (requestId === projectRequestId.current) {
        setProjects(result.items);
        setProjectTotal(result.total);
      }
    } catch (error) {
      if (requestId === projectRequestId.current) {
        setLoadError(getErrorMessage(error));
      }
    } finally {
      if (requestId === projectRequestId.current) {
        setLoading(false);
      }
    }
  }, [projectPage]);

  useEffect(() => {
    void loadProjects();
  }, [loadProjects]);

  function openEdit(project: KnowledgeProject) {
    form.setFieldsValue({
      name: project.name,
      description: project.description,
    });
    setEditing(project);
  }

  async function saveEdit() {
    if (!editing) return;
    try {
      const values = await form.validateFields();
      setSaving(true);
      const updated = await knowledgeCatalogApi.upsertProject(
        projectInput(editing, { ...values, enabled: editing.enabled }),
      );
      setProjects((current) =>
        current.map((project) =>
          project.id === updated.id ? updated : project,
        ),
      );
      message.success("项目已保存");
      setEditing(null);
    } catch (error) {
      if (error && typeof error === "object" && "errorFields" in error) return;
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  }

  function confirmEnabled(project: KnowledgeProject, enabled: boolean) {
    Modal.confirm({
      title: enabled ? "恢复使用这个项目？" : "暂停这个项目？",
      content: enabled
        ? "恢复后，项目会重新出现在日常操作中。"
        : "暂停后会保留历史资料，但不会作为默认操作对象。",
      okText: enabled ? "恢复使用" : "暂停项目",
      cancelText: "取消",
      okButtonProps: enabled ? undefined : { danger: true },
      async onOk() {
        try {
          const updated = await knowledgeCatalogApi.upsertProject(
            projectInput(project, {
              name: project.name,
              description: project.description,
              enabled,
            }),
          );
          setProjects((current) =>
            current.map((item) => (item.id === updated.id ? updated : item)),
          );
          message.success(enabled ? "项目已恢复" : "项目已暂停");
        } catch (error) {
          message.error(getErrorMessage(error));
          throw error;
        }
      },
    });
  }

  function confirmDelete(project: KnowledgeProject) {
    Modal.confirm({
      title: `删除“${project.name}”？`,
      content:
        "删除后不再显示在项目列表中，已保存的历史资料会保留，便于后续恢复。",
      okText: "删除项目",
      cancelText: "取消",
      okButtonProps: { danger: true },
      async onOk() {
        try {
          await knowledgeCatalogApi.deleteProject(project.id);
          setProjects((current) =>
            current.filter((item) => item.id !== project.id),
          );
          setProjectTotal((current) => Math.max(0, current - 1));
          if (projects.length === 1 && projectPage > 1) {
            setProjectPage((current) => current - 1);
          } else {
            void loadProjects();
          }
          message.success("项目已删除");
        } catch (error) {
          message.error(getErrorMessage(error));
          throw error;
        }
      },
    });
  }

  return (
    <main className="w-full px-4 py-6 sm:px-6">
      <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <Title level={2} className="!mb-1">
            项目
          </Title>
          <Text type="secondary">
            从一个项目开始，集中管理代码、文档与项目知识。
          </Text>
        </div>
        <Space wrap>
          <Button
            icon={<RefreshCw size={16} />}
            loading={loading}
            onClick={() => void loadProjects()}
          >
            刷新
          </Button>
          <Button
            type="primary"
            icon={<FolderPlus size={17} />}
            onClick={() => navigate("/knowledge/projects/new")}
          >
            创建项目
          </Button>
        </Space>
      </div>

      {loadError ? (
        <Alert
          className="mb-4"
          type="error"
          showIcon
          title="项目暂时无法读取"
          description={loadError}
          action={
            <Space size="small">
              <Button onClick={() => void loadProjects()}>重试</Button>
            </Space>
          }
        />
      ) : null}

      <Card className="overflow-hidden">
        {loading ? <div className="py-8 text-center">正在读取项目…</div> : null}
        {!loading && projects.length ? (
          <div className="divide-y divide-[var(--border)]">
            {projects.map((project) => (
              <div
                key={project.id}
                className="flex flex-wrap items-center justify-between gap-3 px-1 py-4"
              >
                <div className="min-w-0 flex-1">
                  <Space size={8} wrap>
                    <Button
                      type="link"
                      className="!h-auto !p-0 !text-base !font-semibold"
                      onClick={() =>
                        navigate(`/knowledge/projects/${project.id}/overview`)
                      }
                    >
                      {project.name}
                    </Button>
                    {project.enabled ? (
                      <Tag color="green">使用中</Tag>
                    ) : (
                      <Tag>已暂停</Tag>
                    )}
                  </Space>
                  <Paragraph
                    className="!mb-1 !mt-1"
                    type="secondary"
                    ellipsis={{ rows: 1 }}
                  >
                    {project.description || "尚未添加项目说明"}
                  </Paragraph>
                  <Text type="secondary" className="text-xs">
                    已关联 {project.gitWorkspaceKeys.length} 个代码仓库
                  </Text>
                </div>
                <Space size={0} className="ml-auto">
                  <Button
                    type="text"
                    aria-label={`编辑${project.name}`}
                    icon={<Pencil size={16} />}
                    onClick={() => openEdit(project)}
                  />
                  <Button
                    type="text"
                    aria-label={
                      project.enabled
                        ? `暂停${project.name}`
                        : `恢复${project.name}`
                    }
                    icon={<Power size={16} />}
                    onClick={() => confirmEnabled(project, !project.enabled)}
                  />
                  <Button
                    type="text"
                    danger
                    aria-label={`删除${project.name}`}
                    icon={<Trash2 size={16} />}
                    onClick={() => confirmDelete(project)}
                  />
                  <Button
                    type="text"
                    aria-label={`${project.name}更多操作`}
                    icon={<MoreHorizontal size={17} />}
                    onClick={() =>
                      navigate(`/knowledge/projects/${project.id}/overview`)
                    }
                  />
                </Space>
                <Space size="small" wrap>
                  <Button
                    type="link"
                    aria-label={`进入${project.name}项目问答`}
                    icon={<MessageCircleQuestion size={16} />}
                    onClick={() =>
                      navigate(`/knowledge/projects/${project.id}/qa`)
                    }
                  >
                    项目问答
                  </Button>
                  <Button
                    type="link"
                    icon={<ArrowRight size={16} />}
                    iconPlacement="end"
                    onClick={() =>
                      navigate(`/knowledge/projects/${project.id}/overview`)
                    }
                  >
                    进入项目
                  </Button>
                </Space>
              </div>
            ))}
          </div>
        ) : null}
        {!loading && !loadError && projects.length === 0 ? (
          <div className="py-6 text-center">
            <Empty description="还没有项目" />
            <Button
              type="primary"
              icon={<FolderPlus size={17} />}
              onClick={() => navigate("/knowledge/projects/new")}
            >
              创建第一个项目
            </Button>
          </div>
        ) : null}
        {!loading && projectTotal > PROJECT_PAGE_SIZE ? (
          <div className="flex justify-end border-t border-[var(--border)] px-1 py-4">
            <Pagination
              current={projectPage}
              pageSize={PROJECT_PAGE_SIZE}
              total={projectTotal}
              showSizeChanger={false}
              onChange={(page) => setProjectPage(page)}
            />
          </div>
        ) : null}
      </Card>

      <Drawer
        title="编辑项目"
        open={editing !== null}
        size="default"
        destroyOnHidden
        onClose={() => setEditing(null)}
        extra={
          <Space>
            <Button onClick={() => setEditing(null)}>取消</Button>
            <Button
              type="primary"
              loading={saving}
              onClick={() => void saveEdit()}
            >
              保存
            </Button>
          </Space>
        }
      >
        <Form form={form} layout="vertical" requiredMark="optional">
          <Form.Item
            label="项目名称"
            name="name"
            rules={[
              { required: true, whitespace: true, message: "请输入项目名称" },
            ]}
          >
            <Input autoFocus maxLength={100} placeholder="例如：客户服务平台" />
          </Form.Item>
          <Form.Item label="项目说明" name="description">
            <Input.TextArea
              rows={4}
              maxLength={500}
              showCount
              placeholder="用一句话说明这个项目的用途（可选）"
            />
          </Form.Item>
        </Form>
      </Drawer>
    </main>
  );
}
