import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Alert,
  Button,
  Card,
  Collapse,
  Drawer,
  Empty,
  Form,
  Input,
  message,
  Modal,
  Popconfirm,
  Select,
  Skeleton,
  Space,
  Tag,
  Typography,
} from "antd";
import { ArrowLeft, FileSearch, RefreshCw, Search } from "lucide-react";
import { getErrorCode, getErrorMessage } from "@/lib/api";
import {
  knowledgeCatalogApi,
  knowledgeDocumentsApi,
  knowledgeSearchApi,
  knowledgeTerminologyApi,
} from "@/lib/api/knowledge-domain";
import type {
  KnowledgeCitation,
  KnowledgeCitationDetail,
  KnowledgeProject,
  KnowledgeRelease,
  KnowledgeProjectTerm,
  KnowledgeProjectTermExpansion,
  KnowledgeSearchHit,
} from "@/types";
import { knowledgeDocumentTypeOptions } from "../documentTypes";

const { Paragraph, Text, Title } = Typography;

const channelLabels: Record<string, string> = {
  title: "标题匹配",
  fts: "全文匹配",
  vector: "语义匹配",
  relation: "关系匹配",
};

type SearchSnapshot = {
  query: string;
  releaseId: number | null;
  documentTypes: string[];
};

type TermFormValues = {
  term: string;
  aliasesText: string;
  confirmationNote: string;
};

function hasSameDocumentTypes(left: string[], right: string[]) {
  return (
    left.length === right.length &&
    left.every((documentType, index) => documentType === right[index])
  );
}

/**
 * 将用户输入作为普通文本切片，而不是拼接成 HTML。React 会继续转义每个片段，
 * 因此搜索词或文档摘要中的标签都只能按文本显示，不能作为页面内容执行。
 */
function HighlightedText({ value, query }: { value: string; query: string }) {
  const terms = Array.from(
    new Set(
      query
        .trim()
        .split(/\s+/)
        .filter((term) => term.length > 0 && term.length <= 120),
    ),
  )
    .sort((left, right) => right.length - left.length)
    .slice(0, 12);
  if (!value || terms.length === 0) return <>{value}</>;

  const escapedTerms = terms.map((term) =>
    term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"),
  );
  const pieces = value.split(new RegExp(`(${escapedTerms.join("|")})`, "giu"));
  return (
    <>
      {pieces.map((piece, index) =>
        index % 2 === 1 ? (
          <mark key={`${piece}-${index}`}>{piece}</mark>
        ) : (
          piece
        ),
      )}
    </>
  );
}

function channelLabel(channel: string) {
  return channelLabels[channel] ?? "其他匹配";
}

function sortSummary(hit: KnowledgeSearchHit) {
  const summary = hit.diagnostics.sortSummary;
  if (typeof summary === "string" && summary.trim()) return summary;
  if (hit.channels.includes("title") && hit.channels.includes("fts")) {
    return "标题和正文均匹配，按标题优先显示";
  }
  if (hit.channels.includes("title")) return "标题匹配，优先显示";
  if (hit.channels.includes("fts")) return "正文匹配";
  return "根据相关内容匹配";
}

/** 项目搜索默认锁定项目范围，版本和类型等少用条件收纳在高级筛选中。 */
export default function ProjectSearchPage() {
  const navigate = useNavigate();
  const { projectId } = useParams();
  const numericProjectId = Number(projectId);
  const [project, setProject] = useState<KnowledgeProject | null>(null);
  const [releases, setReleases] = useState<KnowledgeRelease[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [submittedQuery, setSubmittedQuery] = useState("");
  const [submittedFilters, setSubmittedFilters] = useState<
    Omit<SearchSnapshot, "query">
  >({ releaseId: null, documentTypes: [] });
  const [releaseId, setReleaseId] = useState<number | null>(null);
  const [documentTypes, setDocumentTypes] = useState<string[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [ftsRebuildRequired, setFtsRebuildRequired] = useState(false);
  const [rebuildingFts, setRebuildingFts] = useState(false);
  const [hasSearched, setHasSearched] = useState(false);
  const [hits, setHits] = useState<KnowledgeSearchHit[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [snapshotChanged, setSnapshotChanged] = useState(false);
  const [appliedTerms, setAppliedTerms] = useState<
    KnowledgeProjectTermExpansion[]
  >([]);
  const [termsDrawerOpen, setTermsDrawerOpen] = useState(false);
  const [terms, setTerms] = useState<KnowledgeProjectTerm[]>([]);
  const [termsLoading, setTermsLoading] = useState(false);
  const [termsError, setTermsError] = useState<string | null>(null);
  const [termEditorOpen, setTermEditorOpen] = useState(false);
  const [editingTerm, setEditingTerm] = useState<KnowledgeProjectTerm | null>(
    null,
  );
  const [termSaving, setTermSaving] = useState(false);
  const [deletingTermId, setDeletingTermId] = useState<number | null>(null);
  const [termForm] = Form.useForm<TermFormValues>();
  const [messageApi, messageContextHolder] = message.useMessage();
  const [selectedCitation, setSelectedCitation] =
    useState<KnowledgeCitation | null>(null);
  const [citationDetail, setCitationDetail] =
    useState<KnowledgeCitationDetail | null>(null);
  const [citationError, setCitationError] = useState<string | null>(null);
  const [citationLoading, setCitationLoading] = useState(false);
  const activeSearchId = useRef(0);
  const inFlightCursors = useRef(new Set<string>());
  const projectRequestId = useRef(0);
  const citationRequestId = useRef(0);
  const termRequestId = useRef(0);

  const loadProject = useCallback(async () => {
    const requestId = ++projectRequestId.current;
    if (!Number.isSafeInteger(numericProjectId) || numericProjectId < 1) {
      if (requestId === projectRequestId.current) {
        setLoadError("项目地址无效");
        setLoading(false);
      }
      return;
    }
    setLoading(true);
    setLoadError(null);
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
      if (requestId !== projectRequestId.current) return;
      setProject(selectedProject);
      setReleases(projectReleases);
      setReleaseId((current) =>
        current != null && projectReleases.some((item) => item.id === current)
          ? current
          : null,
      );
    } catch (error) {
      if (requestId === projectRequestId.current) {
        setLoadError(getErrorMessage(error));
      }
    } finally {
      if (requestId === projectRequestId.current) {
        setLoading(false);
      }
    }
  }, [numericProjectId]);

  useEffect(() => {
    // 路由项目变化时，让尚未返回的项目、搜索和引用请求全部失效，避免旧范围的
    // 版本或命中短暂混入新项目页面。
    activeSearchId.current += 1;
    inFlightCursors.current.clear();
    citationRequestId.current += 1;
    setProject(null);
    setReleases([]);
    setReleaseId(null);
    setSubmittedQuery("");
    setSubmittedFilters({ releaseId: null, documentTypes: [] });
    setHits([]);
    setNextCursor(null);
    setSnapshotChanged(false);
    setAppliedTerms([]);
    setTerms([]);
    setTermsDrawerOpen(false);
    setTermsError(null);
    setTermEditorOpen(false);
    setSearchError(null);
    setFtsRebuildRequired(false);
    setHasSearched(false);
    setSearching(false);
    closeCitationDetail();
    void loadProject();
    return () => {
      projectRequestId.current += 1;
    };
  }, [loadProject]);

  const loadTerms = useCallback(async () => {
    if (!project) return;
    const requestId = ++termRequestId.current;
    setTermsLoading(true);
    setTermsError(null);
    try {
      const items = await knowledgeTerminologyApi.listProjectTerms(project.id);
      if (requestId === termRequestId.current) setTerms(items);
    } catch (error) {
      if (requestId === termRequestId.current) {
        setTermsError(getErrorMessage(error));
      }
    } finally {
      if (requestId === termRequestId.current) setTermsLoading(false);
    }
  }, [project]);

  function openTermManager() {
    setTermsDrawerOpen(true);
    void loadTerms();
  }

  function openTermEditor(term: KnowledgeProjectTerm | null = null) {
    setEditingTerm(term);
    setTermEditorOpen(true);
  }

  useEffect(() => {
    if (!termEditorOpen) return;
    // 表单已挂载后再写入初始值，避免 Ant Design 在编辑弹窗首次打开时把实例判为未连接。
    termForm.setFieldsValue({
      term: editingTerm?.term ?? "",
      aliasesText: editingTerm?.aliases.join(", ") ?? "",
      confirmationNote: editingTerm?.confirmationNote ?? "",
    });
  }, [editingTerm, termEditorOpen, termForm]);

  async function saveTerm(values: TermFormValues) {
    if (!project) return;
    const aliases = values.aliasesText
      .split(/[,，]/)
      .map((value) => value.trim())
      .filter(Boolean);
    setTermSaving(true);
    try {
      const saved = await knowledgeTerminologyApi.upsertProjectTerm({
        id: editingTerm?.id,
        projectId: project.id,
        term: values.term,
        aliases,
        confirmationNote: values.confirmationNote,
      });
      setTerms((current) => {
        const existing = current.findIndex((item) => item.id === saved.id);
        if (existing < 0) return [...current, saved];
        return current.map((item) => (item.id === saved.id ? saved : item));
      });
      messageApi.success("项目术语已保存");
      setTermEditorOpen(false);
      setEditingTerm(null);
      termForm.resetFields();
    } catch (error) {
      messageApi.error(getErrorMessage(error));
    } finally {
      setTermSaving(false);
    }
  }

  async function deleteTerm(term: KnowledgeProjectTerm) {
    if (!project) return;
    setDeletingTermId(term.id);
    try {
      await knowledgeTerminologyApi.deleteProjectTerm(project.id, term.id);
      setTerms((current) => current.filter((item) => item.id !== term.id));
      messageApi.success("项目术语已删除");
    } catch (error) {
      messageApi.error(getErrorMessage(error));
    } finally {
      setDeletingTermId(null);
    }
  }

  async function search(cursor: string | null = null) {
    // 翻页始终携带首次成功搜索的完整条件。这样用户暂时调整筛选项时，
    // 游标仍与后端快照一致，不会把旧游标误用于新的版本或文档类型范围。
    const snapshot: SearchSnapshot = cursor
      ? { query: submittedQuery, ...submittedFilters }
      : {
          query: query.trim(),
          releaseId,
          documentTypes: [...documentTypes],
        };
    if (!snapshot.query || !project) return;
    const cursorKey = cursor ?? "__first_page__";
    if (cursor && inFlightCursors.current.has(cursorKey)) return;
    const searchId = cursor
      ? activeSearchId.current
      : activeSearchId.current + 1;
    if (!cursor) {
      activeSearchId.current = searchId;
      inFlightCursors.current.clear();
      setSnapshotChanged(false);
      setAppliedTerms([]);
    }
    inFlightCursors.current.add(cursorKey);
    setSearching(true);
    setSearchError(null);
    setFtsRebuildRequired(false);
    try {
      const page = await knowledgeSearchApi.searchCatalog({
        projectId: project.id,
        projectVersionId: snapshot.releaseId,
        query: snapshot.query,
        documentTypes: snapshot.documentTypes,
        cursor,
        limit: 20,
      });
      if (activeSearchId.current !== searchId) return;
      if (page.snapshotChanged) {
        setSnapshotChanged(true);
        setNextCursor(null);
        setAppliedTerms(page.appliedTerms ?? []);
        return;
      }
      setSnapshotChanged(false);
      setAppliedTerms(page.appliedTerms ?? []);
      if (!cursor) {
        setSubmittedQuery(snapshot.query);
        setSubmittedFilters({
          releaseId: snapshot.releaseId,
          documentTypes: snapshot.documentTypes,
        });
      }
      setHits((current) => (cursor ? [...current, ...page.items] : page.items));
      setNextCursor(page.nextCursor);
      setHasSearched(true);
    } catch (error) {
      if (activeSearchId.current !== searchId) return;
      setSearchError(getErrorMessage(error));
      setFtsRebuildRequired(
        getErrorCode(error) === "KNOWLEDGE_FTS_REBUILD_REQUIRED",
      );
      if (!cursor) {
        setHits([]);
        setNextCursor(null);
      }
      setHasSearched(true);
    } finally {
      inFlightCursors.current.delete(cursorKey);
      if (activeSearchId.current === searchId) {
        setSearching(false);
      }
    }
  }

  async function rebuildFtsAndRetrySearch() {
    if (rebuildingFts) return;
    // 重建是较慢的派生数据操作。保留当前搜索世代，避免用户在等待期间切换项目、
    // 筛选或关键词后，旧闭包重新发起搜索并把旧范围结果写入当前页面。
    const rebuildSearchId = activeSearchId.current;
    setRebuildingFts(true);
    try {
      const rebuiltEntries = await knowledgeSearchApi.rebuildFts();
      if (activeSearchId.current !== rebuildSearchId) return;
      messageApi.success(
        `全文索引已重建 ${rebuiltEntries} 条内容，可重新搜索。`,
      );
      await search();
    } catch (error) {
      if (activeSearchId.current === rebuildSearchId) {
        messageApi.error(`全文索引重建失败：${getErrorMessage(error)}`);
      }
    } finally {
      setRebuildingFts(false);
    }
  }

  function closeCitationDetail() {
    citationRequestId.current += 1;
    setSelectedCitation(null);
    setCitationDetail(null);
    setCitationError(null);
    setCitationLoading(false);
  }

  async function openCitationDetail(citation: KnowledgeCitation) {
    if (!citation.chunkId || !citation.documentId) return;
    const requestId = citationRequestId.current + 1;
    citationRequestId.current = requestId;
    setSelectedCitation(citation);
    setCitationDetail(null);
    setCitationError(null);
    setCitationLoading(true);
    try {
      const detail = await knowledgeDocumentsApi.citationDetail(
        citation.chunkId,
      );
      if (detail.document.id !== citation.documentId) {
        throw new Error("引用与搜索结果不一致，请重新搜索");
      }
      if (citationRequestId.current === requestId) setCitationDetail(detail);
    } catch (error) {
      if (citationRequestId.current === requestId) {
        setCitationError(getErrorMessage(error));
      }
    } finally {
      if (citationRequestId.current === requestId) setCitationLoading(false);
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
          title="无法打开项目搜索"
          description={loadError}
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

  const filtersChangedSinceSearch =
    hasSearched &&
    (releaseId !== submittedFilters.releaseId ||
      !hasSameDocumentTypes(documentTypes, submittedFilters.documentTypes));

  return (
    <main className="w-full px-4 py-6 sm:px-6">
      {messageContextHolder}
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
            搜索 {project.name}
          </Title>
          <Paragraph type="secondary" className="!mb-0">
            默认搜索当前项目文档的标题和正文，仅包含每篇文档当前已提交版本。
          </Paragraph>
        </div>
        <Space wrap>
          <Button onClick={openTermManager}>管理项目术语</Button>
          <Button
            icon={<RefreshCw size={16} />}
            onClick={() => void loadProject()}
          >
            刷新版本
          </Button>
        </Space>
      </div>

      <Card>
        <Space orientation="vertical" size={16} className="w-full">
          <Input.Search
            aria-label="搜索关键词"
            value={query}
            enterButton={
              <Button
                type="primary"
                icon={<Search size={16} />}
                loading={searching}
              >
                搜索
              </Button>
            }
            placeholder="输入需求、功能、文件名或关键术语"
            maxLength={200}
            onChange={(event) => setQuery(event.target.value)}
            onSearch={() => void search()}
          />
          <Collapse
            size="small"
            items={[
              {
                key: "filters",
                label: "高级筛选（可选）",
                children: (
                  <div className="grid gap-4 md:grid-cols-2">
                    <label className="flex flex-col gap-1">
                      <Text>项目版本</Text>
                      <Select
                        aria-label="项目版本"
                        allowClear
                        placeholder="全部当前版本"
                        value={releaseId ?? undefined}
                        options={releases.map((release) => ({
                          label: release.version,
                          value: release.id,
                        }))}
                        onChange={(value: number | undefined) =>
                          setReleaseId(value ?? null)
                        }
                      />
                    </label>
                    <label className="flex flex-col gap-1">
                      <Text>文档类型</Text>
                      <Select
                        aria-label="文档类型"
                        allowClear
                        mode="multiple"
                        optionFilterProp="label"
                        placeholder="全部类型"
                        value={documentTypes}
                        options={knowledgeDocumentTypeOptions}
                        onChange={setDocumentTypes}
                      />
                    </label>
                  </div>
                ),
              },
            ]}
          />
        </Space>
      </Card>

      {searchError ? (
        <Alert
          className="mt-4"
          type="error"
          showIcon
          title="搜索失败"
          description={searchError}
          action={
            <Space size="small">
              <Button onClick={() => void search()} disabled={rebuildingFts}>
                重试
              </Button>
              {ftsRebuildRequired ? (
                <Button
                  type="primary"
                  loading={rebuildingFts}
                  onClick={() => void rebuildFtsAndRetrySearch()}
                >
                  重建全文索引
                </Button>
              ) : null}
            </Space>
          }
        />
      ) : null}

      {snapshotChanged ? (
        <Alert
          className="mt-4"
          type="info"
          showIcon
          title="搜索结果已有更新"
          description="新的文档或索引已进入当前范围。刷新后将按最新结果重新排序。"
          action={
            <Button onClick={() => void search()} loading={searching}>
              刷新结果
            </Button>
          }
        />
      ) : null}

      {filtersChangedSinceSearch ? (
        <Alert
          className="mt-4"
          type="info"
          showIcon
          title="筛选条件已修改"
          description="当前结果仍使用上次搜索的筛选条件。请重新搜索后查看新范围的结果。"
        />
      ) : null}

      {hasSearched && appliedTerms.length ? (
        <Alert
          className="mt-4"
          type="info"
          showIcon
          title="已按项目术语扩展检索"
          description={
            <Space wrap>
              {appliedTerms.map((item) => (
                <Tag key={item.term} color="blue">
                  {item.term} → {item.aliases.join("、")}
                </Tag>
              ))}
            </Space>
          }
        />
      ) : null}

      {hasSearched && !searchError && hits.length === 0 ? (
        <Empty className="mt-10" description="没有找到匹配的知识">
          <Text type="secondary">可尝试更短的关键词，或清除高级筛选。</Text>
        </Empty>
      ) : null}

      {hits.length ? (
        <section className="mt-4" aria-label="搜索结果">
          <Text type="secondary" role="status" aria-live="polite">
            已加载 {hits.length} 条相关结果
          </Text>
          <Space orientation="vertical" size={12} className="mt-3 w-full">
            {hits.map((hit) => (
              <Card key={hit.citation.citationKey} size="small">
                <Space orientation="vertical" size={8} className="w-full">
                  <div className="flex flex-wrap items-start justify-between gap-2">
                    <div>
                      <Title level={4} className="!mb-0">
                        <HighlightedText
                          value={hit.citation.title || "未命名文档"}
                          query={submittedQuery}
                        />
                      </Title>
                      <Text type="secondary">
                        {[hit.citation.logicalPath, hit.citation.headingPath]
                          .filter(Boolean)
                          .join(" · ") || "项目知识库"}
                      </Text>
                    </div>
                    <Space size={4} wrap>
                      {hit.channels.map((channel) => (
                        <Tag key={channel}>{channelLabel(channel)}</Tag>
                      ))}
                      {hit.citation.releaseId != null ? (
                        <Tag color="blue">版本 #{hit.citation.releaseId}</Tag>
                      ) : null}
                    </Space>
                  </div>
                  <Paragraph className="!mb-0 whitespace-pre-wrap">
                    <HighlightedText
                      value={
                        hit.citation.excerpt ||
                        hit.content ||
                        "该引用没有可显示的摘要。"
                      }
                      query={submittedQuery}
                    />
                  </Paragraph>
                  <Text type="secondary">{sortSummary(hit)}</Text>
                  {hit.citation.startLine != null ? (
                    <Text type="secondary">
                      引用位置：第 {hit.citation.startLine}
                      {hit.citation.endLine != null
                        ? `–${hit.citation.endLine} 行`
                        : " 行"}
                    </Text>
                  ) : null}
                  {hit.citation.chunkId && hit.citation.documentId ? (
                    <Button
                      type="link"
                      className="!h-auto !px-0"
                      onClick={() => void openCitationDetail(hit.citation)}
                    >
                      查看引用详情
                    </Button>
                  ) : (
                    <Text type="secondary">标题命中，尚未定位到正文段落。</Text>
                  )}
                </Space>
              </Card>
            ))}
          </Space>
          {nextCursor ? (
            <div className="mt-4 text-center">
              <Button
                onClick={() => void search(nextCursor)}
                loading={searching}
                disabled={filtersChangedSinceSearch}
              >
                加载更多
              </Button>
            </div>
          ) : null}
        </section>
      ) : !hasSearched ? (
        <Empty
          className="mt-10"
          image={<FileSearch size={48} aria-hidden="true" />}
          description="输入关键词，开始搜索项目知识"
        />
      ) : null}
      <Drawer
        title="管理项目术语"
        open={termsDrawerOpen}
        onClose={() => {
          if (!termSaving && deletingTermId == null) setTermsDrawerOpen(false);
        }}
        destroyOnHidden
        extra={
          <Button type="primary" onClick={() => openTermEditor()}>
            添加术语
          </Button>
        }
      >
        <Paragraph type="secondary">
          为当前项目确认中文业务词与代码或业务别名。保存后仅在本项目搜索中生效。
        </Paragraph>
        {termsError ? (
          <Alert
            type="error"
            showIcon
            title="无法读取项目术语"
            description={termsError}
            action={<Button onClick={() => void loadTerms()}>重试</Button>}
          />
        ) : null}
        {termsLoading ? <Skeleton active /> : null}
        {!termsLoading && !termsError && terms.length === 0 ? (
          <Empty description="暂无已确认术语" />
        ) : null}
        <Space orientation="vertical" size={12} className="w-full">
          {terms.map((term) => (
            <Card key={term.id} size="small">
              <Space orientation="vertical" size={8} className="w-full">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <Text strong>{term.term}</Text>
                  <Space>
                    <Button size="small" onClick={() => openTermEditor(term)}>
                      编辑
                    </Button>
                    <Popconfirm
                      title="删除项目术语"
                      description="删除后，新搜索将不再使用这条术语映射。"
                      okText="删除"
                      cancelText="取消"
                      okButtonProps={{ loading: deletingTermId === term.id }}
                      onConfirm={() => deleteTerm(term)}
                    >
                      <Button
                        size="small"
                        danger
                        loading={deletingTermId === term.id}
                      >
                        删除
                      </Button>
                    </Popconfirm>
                  </Space>
                </div>
                <div>
                  {term.aliases.map((alias) => (
                    <Tag key={alias}>{alias}</Tag>
                  ))}
                </div>
                <Text type="secondary">确认说明：{term.confirmationNote}</Text>
              </Space>
            </Card>
          ))}
        </Space>
      </Drawer>
      <Modal
        title={editingTerm ? "编辑项目术语" : "添加项目术语"}
        open={termEditorOpen}
        confirmLoading={termSaving}
        okText="保存"
        cancelText="取消"
        onOk={() => termForm.submit()}
        onCancel={() => {
          if (!termSaving) {
            setTermEditorOpen(false);
            setEditingTerm(null);
            termForm.resetFields();
          }
        }}
        destroyOnHidden
        mask={{ closable: false }}
      >
        <Form<TermFormValues>
          form={termForm}
          layout="vertical"
          preserve={false}
          onFinish={(values) => void saveTerm(values)}
        >
          <Form.Item
            name="term"
            label="用户术语"
            rules={[
              { required: true, whitespace: true, message: "请输入用户术语" },
            ]}
          >
            <Input maxLength={80} autoFocus placeholder="例如：工单" />
          </Form.Item>
          <Form.Item
            name="aliasesText"
            label="代码或业务别名"
            extra="可用逗号分隔，例如 WorkOrder, work_order"
            rules={[
              {
                required: true,
                whitespace: true,
                message: "请至少填写一个别名",
              },
            ]}
          >
            <Input maxLength={1500} placeholder="例如：WorkOrder, work_order" />
          </Form.Item>
          <Form.Item
            name="confirmationNote"
            label="确认说明"
            rules={[
              { required: true, whitespace: true, message: "请填写确认说明" },
            ]}
          >
            <Input.TextArea
              maxLength={500}
              showCount
              placeholder="例如：项目负责人已确认该术语对应领域模型"
            />
          </Form.Item>
        </Form>
      </Modal>
      <Drawer
        title="引用详情"
        open={selectedCitation != null}
        onClose={closeCitationDetail}
        destroyOnHidden
      >
        {citationLoading ? <Skeleton active /> : null}
        {citationError ? (
          <Alert
            type="error"
            showIcon
            title="无法打开引用"
            description={citationError}
            action={
              selectedCitation ? (
                <Button
                  onClick={() => void openCitationDetail(selectedCitation)}
                >
                  重试
                </Button>
              ) : null
            }
          />
        ) : null}
        {citationDetail ? (
          <Space orientation="vertical" size={12} className="w-full">
            <div>
              <Text type="secondary">来源文档</Text>
              <Title level={5} className="!mb-0">
                <HighlightedText
                  value={citationDetail.document.title}
                  query={submittedQuery}
                />
              </Title>
            </div>
            <Text type="secondary">
              {[
                citationDetail.citation.logicalPath,
                citationDetail.chunk.headingPath,
              ]
                .filter(Boolean)
                .join(" · ") || "项目知识库"}
            </Text>
            {citationDetail.citation.startLine != null ? (
              <Text type="secondary">
                引用位置：第 {citationDetail.citation.startLine}
                {citationDetail.citation.endLine != null
                  ? `–${citationDetail.citation.endLine} 行`
                  : " 行"}
              </Text>
            ) : null}
            <Paragraph className="!mb-0 whitespace-pre-wrap">
              <HighlightedText
                value={citationDetail.chunk.content}
                query={submittedQuery}
              />
            </Paragraph>
          </Space>
        ) : null}
      </Drawer>
    </main>
  );
}
