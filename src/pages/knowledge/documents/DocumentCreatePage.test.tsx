import { ConfigProvider } from "antd";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes, useNavigate } from "react-router-dom";
import type { KnowledgeProject, KnowledgeRelease } from "@/types";

const openDialog = vi.hoisted(() => vi.fn());
const hasTauriRuntime = vi.hoisted(() => vi.fn());
const knowledgeCatalogApi = vi.hoisted(() => ({
  listProjects: vi.fn(),
  listReleases: vi.fn(),
}));
const knowledgeDocumentsApi = vi.hoisted(() => ({
  saveDraft: vi.fn(),
  commitDraft: vi.fn(),
  restoreVersionToDraft: vi.fn(),
}));
const knowledgeIngestionApi = vi.hoisted(() => ({
  prepareUploadFile: vi.fn(),
  prepareUploadDirectory: vi.fn(),
  createDocumentUpload: vi.fn(),
  createDocumentUploadBatch: vi.fn(),
}));
const aiProviderApi = vi.hoisted(() => ({
  list: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openDialog }));
vi.mock("@/lib/api", () => ({
  getErrorMessage: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
  hasTauriRuntime,
}));
vi.mock("@/lib/api/knowledge-domain", () => ({
  knowledgeCatalogApi,
  knowledgeDocumentsApi,
  knowledgeIngestionApi,
}));
vi.mock("@/lib/api/aiProvider", () => ({ aiProviderApi }));

import DocumentCreatePage from "./DocumentCreatePage";

const project = {
  id: 11,
  projectKey: "customer-platform",
  name: "客户服务平台",
  aliases: [],
  description: "服务客户的统一平台",
  gitWorkspaceKeys: ["gateway"],
  gitWorkspaceKey: "gateway",
  defaultBranch: "main",
  enabled: true,
  createdAt: "2026-08-01T00:00:00Z",
  updatedAt: "2026-08-01T00:00:00Z",
} satisfies KnowledgeProject;

const projectTwo = {
  ...project,
  id: 22,
  projectKey: "billing-platform",
  name: "计费平台",
} satisfies KnowledgeProject;

const releases = [
  {
    id: 31,
    projectId: 11,
    version: "v2.0",
    tagName: "v2.0",
    branch: "main",
    commitSha: "a".repeat(40),
    description: "当前稳定版本",
    releasedAt: "2026-08-01T00:00:00Z",
    createdAt: "2026-08-01T00:00:00Z",
    updatedAt: "2026-08-01T00:00:00Z",
    deletedAt: null,
  },
  {
    id: 30,
    projectId: 11,
    version: "v1.0",
    tagName: "v1.0",
    branch: "main",
    commitSha: "b".repeat(40),
    description: "历史版本",
    releasedAt: "2026-07-01T00:00:00Z",
    createdAt: "2026-07-01T00:00:00Z",
    updatedAt: "2026-07-01T00:00:00Z",
    deletedAt: null,
  },
] satisfies KnowledgeRelease[];

const projectTwoReleases = [
  {
    ...releases[0],
    id: 41,
    projectId: 22,
    version: "v9.0",
    tagName: "v9.0",
  },
] satisfies KnowledgeRelease[];

function renderPage(path = "/knowledge/projects/11/documents/new") {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route
            path="/knowledge/projects/:projectId/documents/new"
            element={<DocumentCreatePage />}
          />
          <Route
            path="/knowledge/projects/:projectId/overview"
            element={<div>项目概览</div>}
          />
          <Route path="/knowledge/projects" element={<div>项目列表</div>} />
        </Routes>
      </MemoryRouter>
    </ConfigProvider>,
  );
}

function ProjectSwitchHarness() {
  const navigate = useNavigate();
  return (
    <>
      <button
        type="button"
        onClick={() => navigate("/knowledge/projects/22/documents/new")}
      >
        切换到项目 22
      </button>
      <DocumentCreatePage />
    </>
  );
}

function renderSwitchablePage() {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={["/knowledge/projects/11/documents/new"]}>
        <Routes>
          <Route
            path="/knowledge/projects/:projectId/documents/new"
            element={<ProjectSwitchHarness />}
          />
        </Routes>
      </MemoryRouter>
    </ConfigProvider>,
  );
}

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

describe("DocumentCreatePage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    hasTauriRuntime.mockReturnValue(true);
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [project],
      total: 1,
      offset: 0,
      limit: 100,
    });
    knowledgeCatalogApi.listReleases.mockResolvedValue(releases);
    knowledgeDocumentsApi.saveDraft.mockResolvedValue({
      conflict: false,
      draft: {
        id: 51,
        documentId: null,
        projectId: 11,
        title: "退款审批说明",
        content: "# 审批规则",
        docType: "markdown",
        baseVersionId: null,
        revision: 1,
        editorLabel: "",
      },
    });
    knowledgeDocumentsApi.commitDraft.mockResolvedValue({
      documentId: 61,
      documentVersionId: 71,
      parentVersionId: null,
      contentHash: "a".repeat(64),
      indexJobId: 81,
      indexJobStatus: "queued",
    });
    knowledgeDocumentsApi.restoreVersionToDraft.mockResolvedValue({
      sourceVersion: { id: 71 },
      draft: {
        id: 51,
        documentId: 61,
        projectId: 11,
        title: "退款审批说明",
        content: "# 审批规则",
        docType: "markdown",
        baseVersionId: 71,
        revision: 1,
        editorLabel: "",
      },
      conflict: false,
    });
    knowledgeIngestionApi.prepareUploadFile.mockResolvedValue({
      fileHandle: "upload-handle-1",
      displayName: "退款审批说明.docx",
      sizeBytes: 2048,
    });
    knowledgeIngestionApi.prepareUploadDirectory.mockResolvedValue({
      directoryName: "退款原型",
      files: [],
      skippedCount: 0,
      totalSizeBytes: 0,
    });
    knowledgeIngestionApi.createDocumentUpload.mockResolvedValue({
      documentId: 61,
      assetId: 71,
      importJobId: 81,
      importJobKey: "document-import-81",
      status: "queued",
    });
    knowledgeIngestionApi.createDocumentUploadBatch.mockResolvedValue({
      items: [],
    });
    aiProviderApi.list.mockResolvedValue([]);
  });

  afterEach(() => cleanup());

  it("在无效项目地址时展示可理解的错误", async () => {
    renderPage("/knowledge/projects/not-a-number/documents/new");

    expect(await screen.findByText("无法打开文档新增页")).toBeVisible();
    expect(screen.getByText("项目地址无效")).toBeVisible();
    expect(knowledgeCatalogApi.listProjects).not.toHaveBeenCalled();
  });

  it("项目路由切换时只采用最新请求返回的版本", async () => {
    const user = userEvent.setup();
    const firstProjectReleases = deferred<KnowledgeRelease[]>();
    const secondProjectReleases = deferred<KnowledgeRelease[]>();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [project, projectTwo],
      total: 2,
      offset: 0,
      limit: 100,
    });
    knowledgeCatalogApi.listReleases.mockImplementation((id: number) =>
      id === project.id
        ? firstProjectReleases.promise
        : secondProjectReleases.promise,
    );
    renderSwitchablePage();

    await waitFor(() =>
      expect(knowledgeCatalogApi.listReleases).toHaveBeenCalledWith(project.id),
    );
    await user.click(screen.getByRole("button", { name: "切换到项目 22" }));
    await waitFor(() =>
      expect(knowledgeCatalogApi.listReleases).toHaveBeenCalledWith(
        projectTwo.id,
      ),
    );

    secondProjectReleases.resolve(projectTwoReleases);
    expect(await screen.findByText("v9.0")).toBeVisible();

    firstProjectReleases.resolve(releases);
    await waitFor(() => {
      expect(screen.getByText("v9.0")).toBeVisible();
      expect(screen.queryByText("v2.0")).not.toBeInTheDocument();
    });
  });

  it("切换项目时清空旧项目已选择的文件", async () => {
    const user = userEvent.setup();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [project, projectTwo],
      total: 2,
      offset: 0,
      limit: 100,
    });
    knowledgeCatalogApi.listReleases.mockImplementation((id: number) =>
      Promise.resolve(id === project.id ? releases : projectTwoReleases),
    );
    openDialog.mockResolvedValue("/tmp/旧项目说明.md");
    knowledgeIngestionApi.prepareUploadFile.mockResolvedValue({
      fileHandle: "old-project-file-handle",
      displayName: "旧项目说明.md",
      sizeBytes: 1024,
    });
    renderSwitchablePage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.click(screen.getByText("上传文件"));
    await user.click(screen.getByRole("button", { name: "选择文件" }));
    expect(await screen.findByText("旧项目说明.md")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "切换到项目 22" }));
    await waitFor(() => {
      expect(screen.queryByText("旧项目说明.md")).not.toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: "选择文件" }),
      ).not.toBeInTheDocument();
    });
    expect(await screen.findByText("v9.0")).toBeVisible();
  });

  it("从项目上下文加载版本，并通过类型化 API 保存 Markdown 草稿", async () => {
    const user = userEvent.setup();
    renderPage();

    expect(
      await screen.findByRole("heading", { name: "添加文档" }),
    ).toBeVisible();
    expect(
      screen.getByText("保存草稿后，提交时会确认关联版本。"),
    ).toBeVisible();
    expect(screen.getByLabelText("关联版本")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "保存草稿" }),
    ).toBeInTheDocument();

    await user.type(screen.getByLabelText("文档标题"), "退款审批说明");
    await user.type(screen.getByLabelText("文档内容"), "# 审批规则");
    await user.click(screen.getByRole("button", { name: "保存草稿" }));

    await waitFor(() =>
      expect(knowledgeDocumentsApi.saveDraft).toHaveBeenCalledWith({
        draftId: null,
        revision: null,
        projectId: 11,
        title: "退款审批说明",
        content: "# 审批规则",
        docType: "markdown",
        editorLabel: null,
      }),
    );
  });

  it("从历史版本创建新草稿，保留原版本并允许再次提交", async () => {
    const user = userEvent.setup();
    renderPage("/knowledge/projects/11/documents/new?restoreVersionId=71");

    expect(
      await screen.findByRole("heading", { name: "恢复并编辑文档" }),
    ).toBeVisible();
    await waitFor(() =>
      expect(knowledgeDocumentsApi.restoreVersionToDraft).toHaveBeenCalledWith({
        sourceVersionId: 71,
      }),
    );
    expect(screen.getByLabelText("文档标题")).toHaveValue("退款审批说明");
    expect(screen.getByLabelText("文档内容")).toHaveValue("# 审批规则");

    await user.type(screen.getByLabelText("文档内容"), "\n补充例外规则");
    await user.click(screen.getByRole("button", { name: "保存草稿" }));
    await waitFor(() =>
      expect(knowledgeDocumentsApi.saveDraft).toHaveBeenCalledWith({
        draftId: 51,
        revision: 1,
        projectId: 11,
        title: "退款审批说明",
        content: "# 审批规则\n补充例外规则",
        docType: "markdown",
        editorLabel: null,
      }),
    );
  });

  it("保存草稿后默认关联项目版本，并提交为不可变正式版本", async () => {
    const user = userEvent.setup();
    renderPage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.type(screen.getByLabelText("文档标题"), "退款审批说明");
    await user.type(screen.getByLabelText("文档内容"), "# 审批规则");
    await user.click(screen.getByRole("button", { name: "保存草稿" }));

    const commitButton = await screen.findByRole("button", {
      name: "提交为正式版本",
    });
    expect(commitButton).toBeEnabled();

    await user.click(commitButton);
    expect(await screen.findByRole("dialog")).toHaveAccessibleName(
      "提交为正式版本",
    );
    expect(screen.getByLabelText("文档版本名称")).toHaveValue("v2.0");
    await user.click(screen.getByRole("button", { name: "确认提交" }));

    await waitFor(() =>
      expect(knowledgeDocumentsApi.commitDraft).toHaveBeenCalledWith({
        draftId: 51,
        revision: 1,
        versionLabel: "v2.0",
        projectVersionId: 31,
        crossVersionScope: null,
        commitMessage: null,
      }),
    );
    expect(await screen.findByText("正式版本已创建")).toBeVisible();
    expect(
      screen.getByText(
        "索引已排队处理；处理完成前，该文档不会出现在搜索结果中。",
      ),
    ).toBeVisible();
  });

  it("无项目版本时手工提交明确使用全部版本范围", async () => {
    const user = userEvent.setup();
    knowledgeCatalogApi.listReleases.mockResolvedValue([]);
    renderPage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.type(screen.getByLabelText("文档标题"), "通用规则");
    await user.type(screen.getByLabelText("文档内容"), "# 全部版本规则");
    await user.click(screen.getByRole("button", { name: "保存草稿" }));

    const commitButton = await screen.findByRole("button", {
      name: "提交为正式版本",
    });
    expect(commitButton).toBeEnabled();
    await user.click(commitButton);
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("适用于项目全部版本");
    expect(within(dialog).getByLabelText("文档版本名称")).toHaveValue(
      "全部版本",
    );
    await user.click(screen.getByRole("button", { name: "确认提交" }));

    await waitFor(() =>
      expect(knowledgeDocumentsApi.commitDraft).toHaveBeenCalledWith({
        draftId: 51,
        revision: 1,
        versionLabel: "全部版本",
        projectVersionId: null,
        crossVersionScope: "project_all_versions",
        commitMessage: null,
      }),
    );
  });

  it("再次保存时携带草稿 ID 与修订号，避免把同一文档拆成多个草稿", async () => {
    const user = userEvent.setup();
    knowledgeDocumentsApi.saveDraft
      .mockResolvedValueOnce({
        conflict: false,
        draft: {
          id: 51,
          documentId: null,
          projectId: 11,
          title: "退款审批说明",
          content: "# 审批规则",
          docType: "markdown",
          baseVersionId: null,
          revision: 1,
          editorLabel: "",
        },
      })
      .mockResolvedValueOnce({
        conflict: false,
        draft: {
          id: 51,
          documentId: null,
          projectId: 11,
          title: "退款审批说明（更新）",
          content: "# 审批规则",
          docType: "markdown",
          baseVersionId: null,
          revision: 2,
          editorLabel: "",
        },
      });
    renderPage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.type(screen.getByLabelText("文档标题"), "退款审批说明");
    await user.type(screen.getByLabelText("文档内容"), "# 审批规则");
    await user.click(screen.getByRole("button", { name: "保存草稿" }));
    await user.type(screen.getByLabelText("文档标题"), "（更新）");
    await user.click(screen.getByRole("button", { name: "保存草稿" }));

    await waitFor(() =>
      expect(knowledgeDocumentsApi.saveDraft).toHaveBeenLastCalledWith({
        draftId: 51,
        revision: 1,
        projectId: 11,
        title: "退款审批说明（更新）",
        content: "# 审批规则",
        docType: "markdown",
        editorLabel: null,
      }),
    );
  });

  it("草稿冲突时保留本地内容并要求先加载服务器草稿，不能直接覆盖", async () => {
    const user = userEvent.setup();
    knowledgeDocumentsApi.saveDraft.mockResolvedValueOnce({
      conflict: true,
      draft: {
        id: 51,
        documentId: null,
        projectId: 11,
        title: "服务器标题",
        content: "服务器当前正文",
        docType: "markdown",
        baseVersionId: null,
        revision: 2,
        editorLabel: "另一位编辑者",
      },
    });
    renderPage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.type(screen.getByLabelText("文档标题"), "本地标题");
    await user.type(screen.getByLabelText("文档内容"), "本地未保存正文");
    await user.click(screen.getByRole("button", { name: "保存草稿" }));

    expect(await screen.findByText("草稿已被其他编辑者更新")).toBeVisible();
    await user.click(screen.getByText("本地未保存内容", { exact: true }));
    expect(screen.getByLabelText("本地未保存内容")).toHaveValue(
      "本地未保存正文",
    );
    expect(screen.getByRole("button", { name: "保存草稿" })).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: "提交为正式版本" }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "加载服务器草稿" }));
    expect(screen.getByLabelText("文档标题")).toHaveValue("服务器标题");
    expect(screen.getByLabelText("文档内容")).toHaveValue("服务器当前正文");
    expect(screen.getByRole("button", { name: "保存草稿" })).toBeEnabled();
  });

  it("保存进行中锁定编辑，防止旧响应覆盖后续输入", async () => {
    const user = userEvent.setup();
    const savedDraftResponse = {
      conflict: false,
      draft: {
        id: 51,
        documentId: null,
        projectId: 11,
        title: "退款审批说明",
        content: "# 审批规则",
        docType: "markdown" as const,
        baseVersionId: null,
        revision: 1,
        editorLabel: "",
      },
    };
    let finishSave: (value: typeof savedDraftResponse) => void = () =>
      undefined;
    knowledgeDocumentsApi.saveDraft.mockReturnValueOnce(
      new Promise<typeof savedDraftResponse>((resolve) => {
        finishSave = resolve;
      }),
    );
    renderPage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.type(screen.getByLabelText("文档标题"), "退款审批说明");
    await user.type(screen.getByLabelText("文档内容"), "# 审批规则");
    await user.click(screen.getByRole("button", { name: "保存草稿" }));

    await waitFor(() =>
      expect(screen.getByLabelText("文档内容")).toBeDisabled(),
    );
    finishSave(savedDraftResponse);
    await waitFor(() =>
      expect(screen.getByLabelText("文档内容")).toBeEnabled(),
    );
    expect(screen.getByLabelText("文档内容")).toHaveValue("# 审批规则");
  });

  it("默认关联最新项目版本，选择文件后可以直接上传", async () => {
    const user = userEvent.setup();
    openDialog.mockResolvedValue("/tmp/退款审批说明.docx");
    renderPage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.click(screen.getByText("上传文件"));
    const uploadButton = screen.getByRole("button", { name: "开始上传" });
    expect(uploadButton).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "选择文件" }));
    await waitFor(() =>
      expect(knowledgeIngestionApi.prepareUploadFile).toHaveBeenCalledWith({
        selectedPath: "/tmp/退款审批说明.docx",
      }),
    );
    expect(await screen.findByText("退款审批说明.docx")).toBeVisible();
    expect(uploadButton).toBeEnabled();
    await user.click(uploadButton);
    await waitFor(() =>
      expect(knowledgeIngestionApi.createDocumentUpload).toHaveBeenCalledWith({
        projectId: 11,
        projectVersionId: 31,
        crossVersionScope: null,
        fileHandle: "upload-handle-1",
        displayName: "退款审批说明.docx",
      }),
    );
  });

  it("项目尚无版本时默认按全部版本上传", async () => {
    const user = userEvent.setup();
    knowledgeCatalogApi.listReleases.mockResolvedValue([]);
    openDialog.mockResolvedValue("/tmp/通用说明.md");
    knowledgeIngestionApi.prepareUploadFile.mockResolvedValue({
      fileHandle: "upload-all-versions-handle",
      displayName: "通用说明.md",
      sizeBytes: 1024,
    });
    renderPage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.click(screen.getByText("上传文件"));
    await user.click(screen.getByRole("button", { name: "选择文件" }));

    const uploadButton = screen.getByRole("button", { name: "开始上传" });
    expect(await screen.findByText("通用说明.md")).toBeVisible();
    expect(uploadButton).toBeEnabled();
    await user.click(uploadButton);

    await waitFor(() =>
      expect(knowledgeIngestionApi.createDocumentUpload).toHaveBeenCalledWith({
        projectId: 11,
        projectVersionId: null,
        crossVersionScope: "project_all_versions",
        fileHandle: "upload-all-versions-handle",
        displayName: "通用说明.md",
      }),
    );
  });

  it("图片没有可用远程视觉服务时仍会优先使用本机文字识别", async () => {
    const user = userEvent.setup();
    openDialog.mockResolvedValue("/tmp/退款流程.png");
    knowledgeIngestionApi.prepareUploadFile.mockResolvedValue({
      fileHandle: "upload-image-handle",
      displayName: "退款流程.png",
      sizeBytes: 1024,
    });
    renderPage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.click(screen.getByText("上传文件"));
    await user.click(screen.getByText("适用于全部版本"));
    await user.click(screen.getByRole("button", { name: "选择文件" }));

    expect(await screen.findByText("可选远程文字识别")).toBeVisible();
    await waitFor(() => expect(aiProviderApi.list).toHaveBeenCalledTimes(1));
    expect(
      screen.getByText(
        "当前没有可用的远程视觉识别服务，仍会优先使用本机文字识别；如需远程识别，请稍后在 AI 服务中配置并测试服务。",
      ),
    ).toBeVisible();
    const uploadButton = screen.getByRole("button", { name: "开始上传" });
    expect(uploadButton).toBeEnabled();
    await user.click(uploadButton);
    await waitFor(() =>
      expect(knowledgeIngestionApi.createDocumentUpload).toHaveBeenCalledWith({
        projectId: 11,
        projectVersionId: null,
        crossVersionScope: "project_all_versions",
        fileHandle: "upload-image-handle",
        displayName: "退款流程.png",
        allowRemoteOcr: false,
        ocrProviderKey: null,
      }),
    );
  });

  it("批量选择文件时逐项准备并通过批量 API 入队", async () => {
    const user = userEvent.setup();
    openDialog.mockResolvedValue(["/tmp/说明.md", "/tmp/流程图.png"]);
    knowledgeIngestionApi.prepareUploadFile
      .mockResolvedValueOnce({
        fileHandle: "upload-markdown-handle",
        displayName: "说明.md",
        sizeBytes: 512,
      })
      .mockResolvedValueOnce({
        fileHandle: "upload-image-handle",
        displayName: "流程图.png",
        sizeBytes: 1024,
      });
    knowledgeIngestionApi.createDocumentUploadBatch.mockResolvedValue({
      items: [
        {
          displayName: "说明.md",
          result: {
            documentId: 61,
            assetId: 71,
            importJobId: 81,
            importJobKey: "document-import-81",
            status: "queued",
          },
          errorMessage: null,
        },
        {
          displayName: "流程图.png",
          result: {
            documentId: 62,
            assetId: 72,
            importJobId: 82,
            importJobKey: "document-import-82",
            status: "queued",
          },
          errorMessage: null,
        },
      ],
    });
    renderPage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.click(screen.getByText("上传文件"));
    await user.click(screen.getByText("适用于全部版本"));
    await user.click(screen.getByRole("button", { name: "选择文件" }));

    expect(await screen.findByText("已选择 2 个文件")).toBeVisible();
    expect(screen.getByText("批量上传将优先在本机处理图片")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "开始上传" }));

    await waitFor(() =>
      expect(
        knowledgeIngestionApi.createDocumentUploadBatch,
      ).toHaveBeenCalledWith({
        projectId: 11,
        projectVersionId: null,
        crossVersionScope: "project_all_versions",
        files: [
          {
            fileHandle: "upload-markdown-handle",
            displayName: "说明.md",
            allowRemoteOcr: false,
            ocrProviderKey: null,
          },
          {
            fileHandle: "upload-image-handle",
            displayName: "流程图.png",
            allowRemoteOcr: false,
            ocrProviderKey: null,
          },
        ],
      }),
    );
    expect(knowledgeIngestionApi.createDocumentUpload).not.toHaveBeenCalled();
  });

  it("选择 HTML 原型文件夹后显示资源摘要并通过批量 API 上传", async () => {
    const user = userEvent.setup();
    const preparedDirectory = {
      directoryName: "退款原型",
      files: [
        {
          fileHandle: "prototype-index-handle",
          displayName: "index.html",
          sizeBytes: 4096,
        },
        {
          fileHandle: "prototype-style-handle",
          displayName: "assets/style.css",
          sizeBytes: 1024,
        },
        {
          fileHandle: "prototype-script-handle",
          displayName: "assets/app.js",
          sizeBytes: 2048,
        },
      ],
      skippedCount: 1,
      totalSizeBytes: 7168,
    };
    openDialog.mockResolvedValue("/tmp/退款原型");
    knowledgeIngestionApi.prepareUploadDirectory.mockResolvedValue(
      preparedDirectory,
    );
    knowledgeIngestionApi.createDocumentUploadBatch.mockResolvedValue({
      items: preparedDirectory.files.map((file, index) => ({
        displayName: file.displayName,
        result: {
          documentId: 100 + index,
          assetId: 200 + index,
          importJobId: 300 + index,
          importJobKey: `document-import-${300 + index}`,
          status: "queued",
        },
        errorMessage: null,
      })),
    });
    renderPage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.click(screen.getByText("上传文件"));
    await user.click(screen.getByRole("button", { name: "选择文件夹" }));

    await waitFor(() => {
      expect(openDialog).toHaveBeenCalledWith({
        multiple: false,
        directory: true,
      });
      expect(knowledgeIngestionApi.prepareUploadDirectory).toHaveBeenCalledWith(
        { selectedPath: "/tmp/退款原型" },
      );
    });
    expect(await screen.findByText("已选择文件夹：退款原型")).toBeVisible();
    expect(screen.getByText("3 个文件，合计 7 KB")).toBeVisible();
    expect(
      screen.getByText("已跳过 1 个不支持或未通过校验的文件"),
    ).toBeVisible();

    await user.click(screen.getByRole("button", { name: "移除" }));
    expect(
      screen.queryByText("已选择文件夹：退款原型"),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始上传" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "选择文件夹" }));
    await screen.findByText("已选择文件夹：退款原型");
    await user.click(screen.getByRole("button", { name: "开始上传" }));

    await waitFor(() =>
      expect(
        knowledgeIngestionApi.createDocumentUploadBatch,
      ).toHaveBeenCalledWith({
        projectId: 11,
        projectVersionId: 31,
        crossVersionScope: null,
        sourceFolderName: "退款原型",
        files: [
          {
            fileHandle: "prototype-index-handle",
            displayName: "index.html",
            allowRemoteOcr: false,
            ocrProviderKey: null,
          },
          {
            fileHandle: "prototype-style-handle",
            displayName: "assets/style.css",
            allowRemoteOcr: false,
            ocrProviderKey: null,
          },
          {
            fileHandle: "prototype-script-handle",
            displayName: "assets/app.js",
            allowRemoteOcr: false,
            ocrProviderKey: null,
          },
        ],
      }),
    );
    expect(knowledgeIngestionApi.createDocumentUpload).not.toHaveBeenCalled();
  });

  it("浏览器环境选择文件夹时显示运行环境提示", async () => {
    const user = userEvent.setup();
    hasTauriRuntime.mockReturnValue(false);
    renderPage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.click(screen.getByText("上传文件"));
    await user.click(screen.getByRole("button", { name: "选择文件夹" }));

    expect(
      await screen.findByText("选择上传文件夹需要在 Tauri 桌面端运行。"),
    ).toBeVisible();
    expect(openDialog).not.toHaveBeenCalled();
  });

  it("保存失败时显示错误且保留用户输入", async () => {
    const user = userEvent.setup();
    knowledgeDocumentsApi.saveDraft.mockRejectedValue(
      new Error("保存服务暂不可用"),
    );
    renderPage();

    await screen.findByRole("heading", { name: "添加文档" });
    await user.type(screen.getByLabelText("文档标题"), "保留的标题");
    await user.type(screen.getByLabelText("文档内容"), "保留的正文");
    await user.click(screen.getByRole("button", { name: "保存草稿" }));

    await waitFor(() =>
      expect(knowledgeDocumentsApi.saveDraft).toHaveBeenCalledTimes(1),
    );
    expect(screen.getByLabelText("文档标题")).toHaveValue("保留的标题");
    expect(screen.getByLabelText("文档内容")).toHaveValue("保留的正文");
  });
});
