import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Alert,
  Button,
  Card,
  Descriptions,
  Drawer,
  Empty,
  Input,
  Modal,
  Select,
  Skeleton,
  Space,
  Table,
  Tag,
  Typography,
  message,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import {
  ArrowLeft,
  FilePlus2,
  FolderOpen,
  History,
  RefreshCw,
  RotateCcw,
  Trash2,
} from "lucide-react";
import { getErrorMessage } from "@/lib/api";
import {
  knowledgeCatalogApi,
  knowledgeDocumentsApi,
} from "@/lib/api/knowledge-domain";
import type {
  KnowledgeDocument,
  KnowledgeDocumentDeletionImpactPreview,
  KnowledgeDocumentDetail,
  KnowledgeDocumentImagePreview,
  KnowledgeProject,
  KnowledgeRelease,
} from "@/types";
import { knowledgeDocumentTypeLabels } from "../documentTypes";

const { Paragraph, Text, Title } = Typography;
const PAGE_SIZE = 20;

const statusLabels: Record<string, string> = {
  active: "可用",
  queued: "等待处理",
  processing: "处理中",
  failed: "处理失败",
  cancelled: "已取消",
};

const sensitivityLabels: Record<string, string> = {
  public: "公开",
  internal: "内部",
  confidential: "保密",
  restricted: "受限",
};

interface UploadedFolderPath {
  folderName: string;
  fileName: string;
}

function logicalPathFileName(logicalPath: string, fallback: string) {
  const parts = logicalPath
    .trim()
    .split(/[\\/]+/)
    .map((part) => part.trim())
    .filter(Boolean);
  return parts[parts.length - 1] ?? fallback;
}

/** 文件夹上传仍按资源拆分为文档，来源类型只信任后端上传关联返回的字段。 */
function getUploadedFolderPath(
  document: Pick<
    KnowledgeDocument,
    "logicalPath" | "title" | "sourceFolderName"
  >,
): UploadedFolderPath | null {
  const folderName = document.sourceFolderName?.trim();
  if (!folderName) return null;

  const parts = document.logicalPath
    .trim()
    .split(/[\\/]+/)
    .map((part) => part.trim())
    .filter(Boolean);
  const fileName =
    parts[0] === "upload-folder" && parts[1] === folderName
      ? parts.slice(2).join("/")
      : logicalPathFileName(document.logicalPath, document.title);
  return {
    folderName,
    fileName: fileName || document.title || "未命名文件",
  };
}

function renderDocumentIdentity(document: KnowledgeDocument, title: string) {
  const folderPath = getUploadedFolderPath(document);
  return (
    <Space align="start" size={8}>
      {folderPath ? <FolderOpen size={18} aria-hidden /> : null}
      <Space orientation="vertical" size={2}>
        <Text strong>{folderPath?.folderName ?? title}</Text>
        <Text type="secondary" className="break-all">
          {folderPath
            ? `包含文件：${folderPath.fileName}`
            : document.logicalPath || "手工添加"}
        </Text>
      </Space>
    </Space>
  );
}

function renderDocumentType(document: KnowledgeDocument) {
  const folderPath = getUploadedFolderPath(document);
  return folderPath ? (
    <Tag icon={<FolderOpen size={14} />}>文件夹</Tag>
  ) : (
    <Tag>
      {knowledgeDocumentTypeLabels[document.docType] ?? document.docType}
    </Tag>
  );
}

/** 项目文档页固定当前项目，避免非技术用户在创建、查找和清理时重复选择范围。 */
export default function ProjectDocumentsPage() {
  const navigate = useNavigate();
  const { projectId } = useParams();
  const numericProjectId = Number(projectId);
  const [project, setProject] = useState<KnowledgeProject | null>(null);
  const [releases, setReleases] = useState<KnowledgeRelease[]>([]);
  const [selectedReleaseId, setSelectedReleaseId] = useState<number | null>(
    null,
  );
  const [keyword, setKeyword] = useState("");
  const [appliedKeyword, setAppliedKeyword] = useState<string | null>(null);
  const [documents, setDocuments] = useState<KnowledgeDocument[]>([]);
  const [documentTotal, setDocumentTotal] = useState(0);
  const [documentPage, setDocumentPage] = useState(1);
  const [contextLoading, setContextLoading] = useState(true);
  const [documentsLoading, setDocumentsLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [documentError, setDocumentError] = useState<string | null>(null);
  const [selectedDetail, setSelectedDetail] =
    useState<KnowledgeDocumentDetail | null>(null);
  const [imagePreview, setImagePreview] =
    useState<KnowledgeDocumentImagePreview | null>(null);
  const [imagePreviewError, setImagePreviewError] = useState<string | null>(
    null,
  );
  const [detailLoading, setDetailLoading] = useState(false);
  const [retryingDocumentId, setRetryingDocumentId] = useState<number | null>(
    null,
  );
  const [deleteCandidate, setDeleteCandidate] =
    useState<KnowledgeDocument | null>(null);
  const [deletePreview, setDeletePreview] =
    useState<KnowledgeDocumentDeletionImpactPreview | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [deletingDocumentId, setDeletingDocumentId] = useState<number | null>(
    null,
  );
  const [recycleOpen, setRecycleOpen] = useState(false);
  const [deletedDocuments, setDeletedDocuments] = useState<KnowledgeDocument[]>(
    [],
  );
  const [deletedDocumentTotal, setDeletedDocumentTotal] = useState(0);
  const [deletedDocumentPage, setDeletedDocumentPage] = useState(1);
  const [deletedLoading, setDeletedLoading] = useState(false);
  const [deletedError, setDeletedError] = useState<string | null>(null);
  const [restoringDocumentId, setRestoringDocumentId] = useState<number | null>(
    null,
  );
  const documentsRequestId = useRef(0);
  const deletedDocumentsRequestId = useRef(0);
  const detailRequestId = useRef(0);
  const contextRequestId = useRef(0);

  const loadContext = useCallback(async () => {
    const requestId = ++contextRequestId.current;
    if (!Number.isSafeInteger(numericProjectId) || numericProjectId < 1) {
      if (requestId === contextRequestId.current) {
        setLoadError("项目地址无效");
        setContextLoading(false);
      }
      return;
    }
    setContextLoading(true);
    setLoadError(null);
    try {
      const [projects, projectReleases] = await Promise.all([
        knowledgeCatalogApi.listProjects({
          projectId: numericProjectId,
          limit: 1,
          offset: 0,
        }),
        knowledgeCatalogApi.listReleases(numericProjectId),
      ]);
      const currentProject = projects.items[0] ?? null;
      if (requestId === contextRequestId.current) {
        setProject(currentProject);
        setReleases(currentProject ? projectReleases : []);
        setSelectedReleaseId((current) =>
          current != null && projectReleases.some((item) => item.id === current)
            ? current
            : null,
        );
      }
    } catch (error) {
      if (requestId === contextRequestId.current) {
        setLoadError(getErrorMessage(error));
      }
    } finally {
      if (requestId === contextRequestId.current) {
        setContextLoading(false);
      }
    }
  }, [numericProjectId]);

  const loadDocuments = useCallback(async () => {
    if (!project) return;
    const requestId = ++documentsRequestId.current;
    setDocumentsLoading(true);
    setDocumentError(null);
    try {
      const page = await knowledgeDocumentsApi.list({
        projectId: project.id,
        releaseId: selectedReleaseId,
        keyword: appliedKeyword,
        offset: (documentPage - 1) * PAGE_SIZE,
        limit: PAGE_SIZE,
      });
      if (requestId === documentsRequestId.current) {
        setDocuments(page.items);
        setDocumentTotal(page.total);
      }
    } catch (error) {
      if (requestId === documentsRequestId.current) {
        setDocumentError(getErrorMessage(error));
      }
    } finally {
      if (requestId === documentsRequestId.current) {
        setDocumentsLoading(false);
      }
    }
  }, [appliedKeyword, documentPage, project, selectedReleaseId]);

  const loadDeletedDocuments = useCallback(async () => {
    if (!project) return;
    const requestId = ++deletedDocumentsRequestId.current;
    setDeletedLoading(true);
    setDeletedError(null);
    try {
      const page = await knowledgeDocumentsApi.listDeleted({
        projectId: project.id,
        releaseId: selectedReleaseId,
        keyword: appliedKeyword,
        offset: (deletedDocumentPage - 1) * PAGE_SIZE,
        limit: PAGE_SIZE,
      });
      if (requestId === deletedDocumentsRequestId.current) {
        setDeletedDocuments(page.items);
        setDeletedDocumentTotal(page.total);
      }
    } catch (error) {
      if (requestId === deletedDocumentsRequestId.current) {
        setDeletedError(getErrorMessage(error));
      }
    } finally {
      if (requestId === deletedDocumentsRequestId.current) {
        setDeletedLoading(false);
      }
    }
  }, [appliedKeyword, deletedDocumentPage, project, selectedReleaseId]);

  useEffect(() => {
    void loadContext();
  }, [loadContext]);

  useEffect(() => {
    void loadDocuments();
  }, [loadDocuments]);

  useEffect(() => {
    if (recycleOpen) void loadDeletedDocuments();
  }, [loadDeletedDocuments, recycleOpen]);

  const showDetail = useCallback(async (document: KnowledgeDocument) => {
    const requestId = ++detailRequestId.current;
    setSelectedDetail(null);
    setImagePreview(null);
    setImagePreviewError(null);
    setDetailLoading(true);
    try {
      const detail = await knowledgeDocumentsApi.detail(document.id);
      if (requestId === detailRequestId.current) {
        setSelectedDetail(detail);
      }
      if (
        detail.document.docType === "image" &&
        detail.processing.contentAvailable
      ) {
        try {
          const preview = await knowledgeDocumentsApi.imagePreview(document.id);
          if (requestId === detailRequestId.current) {
            setImagePreview(preview);
          }
        } catch (error) {
          if (requestId === detailRequestId.current) {
            setImagePreviewError(getErrorMessage(error));
          }
        }
      }
    } catch (error) {
      if (requestId === detailRequestId.current) {
        message.error(getErrorMessage(error));
      }
    } finally {
      if (requestId === detailRequestId.current) {
        setDetailLoading(false);
      }
    }
  }, []);

  const retryDocumentProcessing = useCallback(
    async (document: KnowledgeDocument, detail?: KnowledgeDocumentDetail) => {
      if (retryingDocumentId != null) return;
      setRetryingDocumentId(document.id);
      try {
        const currentDetail =
          detail ?? (await knowledgeDocumentsApi.detail(document.id));
        const task = currentDetail.processing.task;
        if (!task?.jobKey || currentDetail.processing.status !== "failed") {
          throw new Error("当前文档没有可重新处理的失败任务");
        }
        await knowledgeDocumentsApi.retryProcessing(task.jobKey);
        message.success("文档已重新加入处理队列");
        if (selectedDetail?.document.id === document.id) {
          await showDetail(document);
        }
        await loadDocuments();
      } catch (error) {
        message.error(getErrorMessage(error));
      } finally {
        setRetryingDocumentId(null);
      }
    },
    [loadDocuments, retryingDocumentId, selectedDetail, showDetail],
  );

  async function requestDelete(document: KnowledgeDocument) {
    setDeleteCandidate(document);
    setDeletePreview(null);
    setPreviewLoading(true);
    try {
      setDeletePreview(
        await knowledgeDocumentsApi.previewDeletion(document.id),
      );
    } catch (error) {
      const errorMessage = getErrorMessage(error);
      message.error(errorMessage);
      setDeleteCandidate(null);
    } finally {
      setPreviewLoading(false);
    }
  }

  async function confirmDelete() {
    if (!deleteCandidate || !deletePreview) return;
    setDeletingDocumentId(deleteCandidate.id);
    try {
      await knowledgeDocumentsApi.softDelete(deleteCandidate.id);
      setDocuments((current) =>
        current.filter((item) => item.id !== deleteCandidate.id),
      );
      setDocumentTotal((current) => Math.max(0, current - 1));
      setDeleteCandidate(null);
      setDeletePreview(null);
      message.success("文档已删除，可在回收站恢复；历史版本与受管资产会保留。");
      if (documents.length === 1 && documentPage > 1) {
        setDocumentPage((current) => current - 1);
      } else {
        void loadDocuments();
      }
      if (recycleOpen) void loadDeletedDocuments();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setDeletingDocumentId(null);
    }
  }

  async function restoreDocument(document: KnowledgeDocument) {
    setRestoringDocumentId(document.id);
    try {
      const result = await knowledgeDocumentsApi.restore(document.id);
      setDeletedDocuments((current) =>
        current.filter((item) => item.id !== document.id),
      );
      setDeletedDocumentTotal((current) => Math.max(0, current - 1));
      message.success(
        `文档已恢复，已重建 ${result.rebuiltFtsEntries} 条全文索引。`,
      );
      if (deletedDocuments.length === 1 && deletedDocumentPage > 1) {
        setDeletedDocumentPage((current) => current - 1);
      } else if (recycleOpen) {
        await loadDeletedDocuments();
      }
      await loadDocuments();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setRestoringDocumentId(null);
    }
  }

  function searchDocuments() {
    const nextKeyword = keyword.trim() || null;
    if (nextKeyword === appliedKeyword) {
      void loadDocuments();
      return;
    }
    setDocumentPage(1);
    setDeletedDocumentPage(1);
    setAppliedKeyword(nextKeyword);
  }

  const columns = useMemo<ColumnsType<KnowledgeDocument>>(
    () => [
      {
        title: "文档",
        dataIndex: "title",
        key: "title",
        render: (title: string, document) =>
          renderDocumentIdentity(document, title),
      },
      {
        title: "类型",
        dataIndex: "docType",
        key: "docType",
        render: (_docType: string, document) => renderDocumentType(document),
      },
      {
        title: "处理状态",
        dataIndex: "status",
        key: "status",
        render: (status: string) => (
          <Tag color={status === "active" ? "green" : undefined}>
            {statusLabels[status] ?? status}
          </Tag>
        ),
      },
      {
        title: "最近更新",
        dataIndex: "updatedAt",
        key: "updatedAt",
        responsive: ["md"],
      },
      {
        title: "操作",
        key: "actions",
        width: 300,
        render: (_, document) => (
          <Space wrap>
            <Button
              type="link"
              icon={<History size={16} />}
              onClick={() => void showDetail(document)}
            >
              查看详情/历史
            </Button>
            {document.status === "failed" ? (
              <Button
                type="link"
                icon={<RotateCcw size={16} />}
                loading={retryingDocumentId === document.id}
                disabled={
                  retryingDocumentId != null &&
                  retryingDocumentId !== document.id
                }
                onClick={() => void retryDocumentProcessing(document)}
              >
                重新处理
              </Button>
            ) : null}
            <Button
              type="link"
              danger
              icon={<Trash2 size={16} />}
              aria-label={`删除${document.title}`}
              onClick={() => void requestDelete(document)}
            >
              删除
            </Button>
          </Space>
        ),
      },
    ],
    [retryDocumentProcessing, retryingDocumentId, showDetail],
  );

  if (contextLoading) {
    return <Skeleton active className="mt-8 w-full px-6" />;
  }

  if (loadError) {
    return (
      <main className="mt-8 w-full px-6">
        <Alert
          type="error"
          showIcon
          title="无法打开项目文档"
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
        返回项目概览
      </Button>
      <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <Title level={2} className="!mb-1">
            {project.name}的文档
          </Title>
          <Paragraph type="secondary" className="!mb-0">
            版本筛选只显示已绑定到该项目版本的文档；删除不会清除历史版本或受管资产。
          </Paragraph>
        </div>
        <Space wrap>
          <Button
            icon={<RotateCcw size={16} />}
            onClick={() => setRecycleOpen(true)}
          >
            回收站
          </Button>
          <Button
            icon={<RefreshCw size={16} />}
            loading={documentsLoading}
            onClick={() => void loadDocuments()}
          >
            刷新
          </Button>
          <Button
            type="primary"
            icon={<FilePlus2 size={16} />}
            onClick={() =>
              navigate(`/knowledge/projects/${project.id}/documents/new`)
            }
          >
            添加文档
          </Button>
        </Space>
      </div>

      <Card className="mb-4">
        <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_240px]">
          <Input.Search
            aria-label="文档标题关键词"
            value={keyword}
            placeholder="按标题、文件路径或标签搜索"
            enterButton="搜索"
            onChange={(event) => setKeyword(event.target.value)}
            onSearch={searchDocuments}
          />
          <label className="flex flex-col gap-1">
            <Text>项目版本</Text>
            <Select
              aria-label="项目版本"
              allowClear
              placeholder="全部版本"
              value={selectedReleaseId ?? undefined}
              options={releases.map((release) => ({
                label: release.version,
                value: release.id,
              }))}
              onChange={(value: number | undefined) => {
                setDocumentPage(1);
                setDeletedDocumentPage(1);
                setSelectedReleaseId(value ?? null);
              }}
            />
          </label>
        </div>
      </Card>

      {documentError ? (
        <Alert
          className="mb-4"
          type="error"
          showIcon
          title="文档暂时无法读取"
          description={documentError}
          action={<Button onClick={() => void loadDocuments()}>重试</Button>}
        />
      ) : null}

      <Card styles={{ body: { padding: 0 } }}>
        <Table<KnowledgeDocument>
          rowKey="id"
          columns={columns}
          dataSource={documents}
          loading={documentsLoading}
          pagination={{
            current: documentPage,
            pageSize: PAGE_SIZE,
            total: documentTotal,
            showSizeChanger: false,
            hideOnSinglePage: true,
            onChange: (page) => setDocumentPage(page),
          }}
          scroll={{ x: 860 }}
          locale={{
            emptyText: documentError ? "" : "当前范围内还没有文档",
          }}
        />
      </Card>

      <Modal
        title={`删除“${deletePreview?.title ?? deleteCandidate?.title ?? "文档"}”？`}
        open={deleteCandidate != null}
        okText="确认删除"
        cancelText="取消"
        okButtonProps={{ danger: true, disabled: !deletePreview }}
        confirmLoading={deletingDocumentId != null}
        onOk={() => void confirmDelete()}
        onCancel={() => {
          if (deletingDocumentId == null) {
            setDeleteCandidate(null);
            setDeletePreview(null);
          }
        }}
        destroyOnHidden
      >
        {previewLoading ? <Skeleton active paragraph={{ rows: 4 }} /> : null}
        {deletePreview ? (
          <Space orientation="vertical" size={12} className="w-full">
            <Text>此次删除只会隐藏该文档，并移除其全文索引。</Text>
            <Descriptions size="small" column={2}>
              <Descriptions.Item label="历史版本">
                {deletePreview.versionCount} 个历史版本
              </Descriptions.Item>
              <Descriptions.Item label="文档分块">
                {deletePreview.chunkCount} 个
              </Descriptions.Item>
              <Descriptions.Item label="向量数据">
                {deletePreview.vectorCount} 条
              </Descriptions.Item>
              <Descriptions.Item label="知识关系">
                {deletePreview.relationCount} 条
              </Descriptions.Item>
              <Descriptions.Item label="受管资产">
                {deletePreview.assetCount} 个
              </Descriptions.Item>
              <Descriptions.Item label="全文索引">
                {deletePreview.ftsEntryCount} 条全文索引
              </Descriptions.Item>
            </Descriptions>
            <Alert
              type="info"
              showIcon
              message={deletePreview.permanentDeletionBlockReason}
            />
          </Space>
        ) : null}
      </Modal>

      <Drawer
        title="文档详情与历史"
        open={detailLoading || selectedDetail != null}
        size="large"
        onClose={() => {
          detailRequestId.current += 1;
          setDetailLoading(false);
          setSelectedDetail(null);
          setImagePreview(null);
          setImagePreviewError(null);
        }}
      >
        {detailLoading ? <Skeleton active /> : null}
        {selectedDetail ? (
          <Space orientation="vertical" size={18} className="w-full">
            <Descriptions column={1} size="small" bordered>
              <Descriptions.Item label="标题">
                {selectedDetail.document.title}
              </Descriptions.Item>
              <Descriptions.Item label="路径">
                {(() => {
                  const folderPath = getUploadedFolderPath(
                    selectedDetail.document,
                  );
                  return folderPath
                    ? `${folderPath.folderName} / ${folderPath.fileName}`
                    : selectedDetail.document.logicalPath || "手工添加";
                })()}
              </Descriptions.Item>
              <Descriptions.Item label="文档类型">
                {renderDocumentType(selectedDetail.document)}
              </Descriptions.Item>
              <Descriptions.Item label="敏感级别">
                {sensitivityLabels[selectedDetail.document.sensitivity] ??
                  selectedDetail.document.sensitivity}
              </Descriptions.Item>
              <Descriptions.Item label="处理状态">
                {statusLabels[selectedDetail.processing.status] ??
                  selectedDetail.processing.status}
              </Descriptions.Item>
            </Descriptions>
            {selectedDetail.processing.status === "failed" ? (
              <Alert
                type="error"
                showIcon
                title="处理失败"
                description={
                  selectedDetail.processing.failureReason ??
                  selectedDetail.processing.message
                }
                action={
                  selectedDetail.processing.task?.jobKey ? (
                    <Button
                      size="small"
                      type="primary"
                      danger
                      icon={<RotateCcw size={14} />}
                      loading={
                        retryingDocumentId === selectedDetail.document.id
                      }
                      disabled={
                        retryingDocumentId != null &&
                        retryingDocumentId !== selectedDetail.document.id
                      }
                      onClick={() =>
                        void retryDocumentProcessing(
                          selectedDetail.document,
                          selectedDetail,
                        )
                      }
                    >
                      重新处理
                    </Button>
                  ) : null
                }
              />
            ) : null}
            {selectedDetail.processing.parser?.warnings.length ? (
              <Alert
                type="warning"
                showIcon
                title="处理提示"
                description={selectedDetail.processing.parser.warnings.join(
                  "；",
                )}
              />
            ) : null}
            {selectedDetail.document.docType === "image" ? (
              <section aria-label="图片预览">
                <Title level={5}>图片预览</Title>
                {imagePreview ? (
                  <Space orientation="vertical" size={8} className="w-full">
                    <img
                      className="max-h-96 max-w-full rounded border object-contain"
                      src={imagePreview.dataUrl}
                      alt={`${selectedDetail.document.title}预览`}
                    />
                    <Text type="secondary">
                      {imagePreview.mimeType} ·{" "}
                      {formatFileSize(imagePreview.sizeBytes)}
                      {imagePreview.width != null && imagePreview.height != null
                        ? ` · ${imagePreview.width} × ${imagePreview.height}`
                        : " · 未识别尺寸"}
                    </Text>
                  </Space>
                ) : imagePreviewError ? (
                  <Alert
                    type="info"
                    showIcon
                    title="暂不能显示图片预览"
                    description={imagePreviewError}
                  />
                ) : (
                  <Alert
                    type="info"
                    showIcon
                    title="图片正文未提取"
                    description="本机文字识别不可用或未识别到文字时，仍可按标题和图片元数据搜索。"
                  />
                )}
              </section>
            ) : null}
            <section aria-label="文档历史版本">
              <Title level={5}>历史版本</Title>
              {selectedDetail.versions.length ? (
                <Space orientation="vertical" className="w-full">
                  {selectedDetail.versions.map((version) => (
                    <Card key={version.id} size="small">
                      <Space orientation="vertical" size={2}>
                        <Text strong>{version.versionLabel}</Text>
                        <Text type="secondary">
                          {version.gitBranch || "手工版本"}
                          {version.commitSha ? ` · ${version.commitSha}` : ""}
                        </Text>
                        <Text type="secondary">{version.createdAt}</Text>
                        <Button
                          type="link"
                          className="!h-auto !px-0"
                          onClick={() =>
                            navigate(
                              `/knowledge/projects/${project.id}/documents/new?restoreVersionId=${version.id}`,
                            )
                          }
                        >
                          恢复并编辑
                        </Button>
                      </Space>
                    </Card>
                  ))}
                </Space>
              ) : (
                <Empty
                  image={Empty.PRESENTED_IMAGE_SIMPLE}
                  description="尚无历史版本"
                />
              )}
            </section>
          </Space>
        ) : null}
      </Drawer>

      <Drawer
        title="回收站"
        open={recycleOpen}
        size={520}
        onClose={() => setRecycleOpen(false)}
      >
        <Paragraph type="secondary">
          已删除文档仍会保留历史版本与受管资产；恢复后会重建全文索引。
        </Paragraph>
        {deletedError ? (
          <Alert
            className="mb-4"
            type="error"
            showIcon
            title="回收站暂时无法读取"
            description={deletedError}
            action={
              <Button onClick={() => void loadDeletedDocuments()}>重试</Button>
            }
          />
        ) : null}
        <Table<KnowledgeDocument>
          rowKey="id"
          loading={deletedLoading}
          dataSource={deletedDocuments}
          pagination={{
            current: deletedDocumentPage,
            pageSize: PAGE_SIZE,
            total: deletedDocumentTotal,
            showSizeChanger: false,
            hideOnSinglePage: true,
            onChange: (page) => setDeletedDocumentPage(page),
          }}
          locale={{ emptyText: deletedError ? "" : "回收站为空" }}
          columns={[
            {
              title: "文档",
              dataIndex: "title",
              key: "title",
              render: (title: string, document) =>
                renderDocumentIdentity(document, title),
            },
            {
              title: "操作",
              key: "actions",
              width: 100,
              render: (_, document) => (
                <Button
                  type="link"
                  loading={restoringDocumentId === document.id}
                  aria-label={`恢复${document.title}`}
                  onClick={() => void restoreDocument(document)}
                >
                  恢复
                </Button>
              ),
            },
          ]}
        />
      </Drawer>
    </main>
  );
}

function formatFileSize(sizeBytes: number) {
  if (sizeBytes < 1024) return `${sizeBytes} B`;
  if (sizeBytes < 1024 * 1024) return `${(sizeBytes / 1024).toFixed(1)} KB`;
  return `${(sizeBytes / (1024 * 1024)).toFixed(1)} MB`;
}
