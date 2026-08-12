import { ConfigProvider, Modal } from "antd";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import type { KnowledgeDocumentDetail, KnowledgeListInput } from "@/types";

const knowledgeCatalogApi = vi.hoisted(() => ({
  listProjects: vi.fn(),
  listReleases: vi.fn(),
}));
const knowledgeDocumentsApi = vi.hoisted(() => ({
  list: vi.fn(),
  listDeleted: vi.fn(),
  detail: vi.fn(),
  imagePreview: vi.fn(),
  previewDeletion: vi.fn(),
  softDelete: vi.fn(),
  restore: vi.fn(),
  retryProcessing: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  getErrorMessage: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
}));
vi.mock("@/lib/api/knowledge-domain", () => ({
  knowledgeCatalogApi,
  knowledgeDocumentsApi,
}));

import ProjectDocumentsPage from "./ProjectDocumentsPage";

const project = {
  id: 11,
  projectKey: "customer-platform",
  name: "客户服务平台",
  aliases: [],
  description: "",
  gitWorkspaceKeys: ["gateway"],
  gitWorkspaceKey: "gateway",
  defaultBranch: "main",
  enabled: true,
  createdAt: "2026-08-01T00:00:00Z",
  updatedAt: "2026-08-01T00:00:00Z",
};

const sampleDocument = {
  id: 31,
  documentKey: "order-guide",
  projectId: 11,
  sourceId: null,
  docType: "markdown",
  title: "订单创建说明",
  logicalPath: "docs/orders.md",
  status: "active",
  sensitivity: "internal",
  tags: ["订单"],
  latestVersionId: 41,
  allowAi: true,
  allowMcp: false,
  createdAt: "2026-08-01T00:00:00Z",
  updatedAt: "2026-08-01T00:00:00Z",
};

function documentDetail(
  targetDocument: typeof sampleDocument,
  versionLabel: string,
): KnowledgeDocumentDetail {
  return {
    document: targetDocument,
    versions: [
      {
        id: targetDocument.latestVersionId,
        documentId: targetDocument.id,
        releaseId: 21,
        versionLabel,
        gitBranch: "main",
        commitSha: "abc123",
        sourcePath: targetDocument.logicalPath,
        mimeType: "text/markdown",
        content: "",
        contentHash: "hash",
        parsedMeta: {},
        tokenEstimate: 1,
        valid: true,
        createdAt: "2026-08-01T00:00:00Z",
      },
    ],
    processing: {
      status: "active",
      message: "可用",
      contentAvailable: true,
      availableActions: [],
    },
  };
}

function renderPage() {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={["/knowledge/projects/11/documents"]}>
        <Routes>
          <Route
            path="/knowledge/projects/:projectId/documents"
            element={<ProjectDocumentsPage />}
          />
          <Route
            path="/knowledge/projects/:projectId/overview"
            element={<div>项目概览</div>}
          />
          <Route
            path="/knowledge/projects/:projectId/documents/new"
            element={<div>添加文档页</div>}
          />
        </Routes>
      </MemoryRouter>
    </ConfigProvider>,
  );
}

async function findDrawer(title: string): Promise<HTMLElement> {
  const titleElements = await screen.findAllByText(title, { exact: true });
  const drawer = titleElements
    .map((element) => element.closest(".ant-drawer"))
    .find((element): element is HTMLElement => element != null);
  expect(drawer).toBeDefined();
  return drawer!;
}

describe("ProjectDocumentsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [project],
      total: 1,
      offset: 0,
      limit: 20,
    });
    knowledgeCatalogApi.listReleases.mockResolvedValue([
      { id: 21, projectId: 11, version: "v1.0.0" },
    ]);
    knowledgeDocumentsApi.list.mockResolvedValue({
      items: [sampleDocument],
      total: 1,
      offset: 0,
      limit: 20,
    });
    knowledgeDocumentsApi.listDeleted.mockResolvedValue({
      items: [sampleDocument],
      total: 1,
      offset: 0,
      limit: 20,
    });
    knowledgeDocumentsApi.detail.mockResolvedValue(
      documentDetail(sampleDocument, "默认历史"),
    );
    knowledgeDocumentsApi.imagePreview.mockResolvedValue({
      documentId: 31,
      mimeType: "image/png",
      sizeBytes: 1024,
      width: 1024,
      height: 768,
      dataUrl: "data:image/png;base64,cHJldmlldw==",
    });
    knowledgeDocumentsApi.previewDeletion.mockResolvedValue({
      documentId: 31,
      title: "订单创建说明",
      versionCount: 2,
      chunkCount: 8,
      vectorCount: 8,
      relationCount: 3,
      assetCount: 1,
      ftsEntryCount: 8,
      permanentDeletionEnabled: false,
      permanentDeletionBlockReason: "永久删除未开放。",
    });
    knowledgeDocumentsApi.softDelete.mockResolvedValue(undefined);
    knowledgeDocumentsApi.restore.mockResolvedValue({
      document: sampleDocument,
      rebuiltFtsEntries: 8,
    });
    knowledgeDocumentsApi.retryProcessing.mockResolvedValue({
      id: 91,
      jobKey: "upload-import-failed-document",
      jobType: "upload_import",
      status: "queued",
      progressCurrent: 0,
      progressTotal: 1,
      message: "任务已进入重试队列",
      checkpoint: {},
      cancelRequested: false,
      startedAt: "2026-08-01T00:00:00Z",
    });
  });

  afterEach(() => {
    Modal.destroyAll();
    cleanup();
  });

  it("锁定项目范围，按版本和标题读取文档", async () => {
    const user = userEvent.setup();
    renderPage();

    expect(await screen.findByText("客户服务平台的文档")).toBeInTheDocument();
    expect(knowledgeDocumentsApi.list).toHaveBeenCalledWith({
      projectId: 11,
      releaseId: null,
      keyword: null,
      offset: 0,
      limit: 20,
    });
    await user.click(screen.getByLabelText("项目版本"));
    await user.click(await screen.findByText("v1.0.0"));
    await waitFor(() =>
      expect(knowledgeDocumentsApi.list).toHaveBeenLastCalledWith({
        projectId: 11,
        releaseId: 21,
        keyword: null,
        offset: 0,
        limit: 20,
      }),
    );
    await user.type(screen.getByLabelText("文档标题关键词"), "订单");
    await user.click(screen.getByRole("button", { name: /搜\s*索/ }));
    await waitFor(() =>
      expect(knowledgeDocumentsApi.list).toHaveBeenLastCalledWith({
        projectId: 11,
        releaseId: 21,
        keyword: "订单",
        offset: 0,
        limit: 20,
      }),
    );
    expect(screen.getByText("订单创建说明")).toBeInTheDocument();
  });

  it("文件夹上传资源在文档列表中显示为文件夹", async () => {
    const folderDocument = {
      ...sampleDocument,
      id: 34,
      documentKey: "refund-prototype-index",
      docType: "html",
      title: "index",
      logicalPath: "upload-folder/退款原型/index.html",
      sourceFolderName: "退款原型",
    };
    knowledgeDocumentsApi.list.mockResolvedValue({
      items: [folderDocument],
      total: 1,
      offset: 0,
      limit: 20,
    });
    renderPage();

    expect(await screen.findByText("退款原型")).toBeInTheDocument();
    expect(screen.getByText("包含文件：index.html")).toBeInTheDocument();
    expect(screen.getByText("文件夹")).toBeInTheDocument();
    expect(screen.queryByText("HTML")).not.toBeInTheDocument();
  });

  it("普通文档即使使用相似路径也不会被误判为文件夹", async () => {
    const ordinaryDocument = {
      ...sampleDocument,
      id: 35,
      logicalPath: "upload-folder/普通目录/readme.md",
      sourceFolderName: null,
    };
    knowledgeDocumentsApi.list.mockResolvedValue({
      items: [ordinaryDocument],
      total: 1,
      offset: 0,
      limit: 20,
    });
    renderPage();

    expect(await screen.findByText("订单创建说明")).toBeInTheDocument();
    expect(screen.queryByText("文件夹")).not.toBeInTheDocument();
  });

  it("失败文档展示可理解原因并允许重新处理", async () => {
    const user = userEvent.setup();
    const failedDocument = {
      ...sampleDocument,
      status: "failed",
      latestVersionId: null,
    };
    knowledgeDocumentsApi.list.mockResolvedValue({
      items: [failedDocument],
      total: 1,
      offset: 0,
      limit: 20,
    });
    knowledgeDocumentsApi.detail.mockResolvedValue({
      document: failedDocument,
      versions: [],
      processing: {
        status: "failed",
        message: "文档处理失败，可重新尝试；不会显示不完整正文",
        failureReason:
          "Markdown Front Matter 格式不正确：did not find expected key at line 3 column 50。Markdown 链接等特殊值请使用引号。",
        contentAvailable: false,
        availableActions: ["重新尝试"],
        task: {
          id: 91,
          jobKey: "upload-import-failed-document",
          jobType: "upload_import",
          status: "failed",
          progressCurrent: 0,
          progressTotal: 1,
          message: "上传文档导入失败",
          cancelRequested: false,
        },
      },
    } satisfies KnowledgeDocumentDetail);
    renderPage();

    await user.click(
      await screen.findByRole("button", { name: "查看详情/历史" }),
    );
    const drawer = await findDrawer("文档详情与历史");
    expect(
      within(drawer).getByText(/Markdown Front Matter 格式不正确/),
    ).toBeInTheDocument();

    await user.click(within(drawer).getByRole("button", { name: "重新处理" }));
    await waitFor(() =>
      expect(knowledgeDocumentsApi.retryProcessing).toHaveBeenCalledWith(
        "upload-import-failed-document",
      ),
    );
  });

  it("删除前展示影响范围，并提供恢复入口", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("订单创建说明");

    await user.click(screen.getByRole("button", { name: "删除订单创建说明" }));
    expect(await screen.findByText("删除“订单创建说明”？")).toBeInTheDocument();
    expect(screen.getByText("2 个历史版本")).toBeInTheDocument();
    expect(screen.getByText("8 条全文索引")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "确认删除" }));

    await waitFor(() =>
      expect(knowledgeDocumentsApi.softDelete).toHaveBeenCalledWith(31),
    );
    await user.click(screen.getByRole("button", { name: "回收站" }));
    await waitFor(() =>
      expect(knowledgeDocumentsApi.listDeleted).toHaveBeenCalledWith({
        projectId: 11,
        releaseId: null,
        keyword: null,
        offset: 0,
        limit: 20,
      }),
    );
    await user.click(screen.getByRole("button", { name: "恢复订单创建说明" }));
    await waitFor(() =>
      expect(knowledgeDocumentsApi.restore).toHaveBeenCalledWith(31),
    );
  });

  it("图片详情加载受控预览，并说明未启用 OCR 的可搜索范围", async () => {
    const user = userEvent.setup();
    const imageDocument = {
      ...sampleDocument,
      id: 33,
      documentKey: "refund-flow-image",
      docType: "image",
      title: "退款流程图",
      logicalPath: "upload/退款流程图.png",
      latestVersionId: 43,
    };
    knowledgeDocumentsApi.list.mockResolvedValue({
      items: [imageDocument],
      total: 1,
      offset: 0,
      limit: 20,
    });
    const imageDetail = documentDetail(imageDocument, "图片版本");
    imageDetail.processing.parser = {
      parserId: "image-metadata-parser-v1",
      parserVersion: "v1",
      qualityLevel: "partial",
      warnings: ["未启用 OCR，图片仅支持标题和元数据搜索"],
    };
    knowledgeDocumentsApi.detail.mockResolvedValue(imageDetail);
    knowledgeDocumentsApi.imagePreview.mockResolvedValue({
      documentId: 33,
      mimeType: "image/png",
      sizeBytes: 1024,
      width: 1024,
      height: 768,
      dataUrl: "data:image/png;base64,cHJldmlldw==",
    });
    renderPage();

    await user.click(
      await screen.findByRole("button", { name: "查看详情/历史" }),
    );
    const drawer = await findDrawer("文档详情与历史");
    expect(await within(drawer).findByText("图片预览")).toBeInTheDocument();
    expect(
      await within(drawer).findByText("未启用 OCR，图片仅支持标题和元数据搜索"),
    ).toBeInTheDocument();
    expect(knowledgeDocumentsApi.imagePreview).toHaveBeenCalledWith(33);
    expect(
      await within(drawer).findByRole("img", { name: "退款流程图预览" }),
    ).toHaveAttribute("src", "data:image/png;base64,cHJldmlldw==");
  });

  it("从历史版本进入恢复编辑路径，不在详情页直接改写正式版本", async () => {
    const user = userEvent.setup();
    renderPage();

    await user.click(
      await screen.findByRole("button", { name: "查看详情/历史" }),
    );
    const drawer = await findDrawer("文档详情与历史");
    await user.click(
      within(drawer).getByRole("button", { name: "恢复并编辑" }),
    );

    expect(await screen.findByText("添加文档页")).toBeInTheDocument();
  });

  it("详情请求乱序返回时保留最后点击的文档", async () => {
    const user = userEvent.setup();
    const secondDocument = {
      ...sampleDocument,
      id: 32,
      documentKey: "refund-guide",
      title: "退款说明",
      logicalPath: "docs/refunds.md",
      latestVersionId: 42,
    };
    let resolveFirstDetail: (detail: KnowledgeDocumentDetail) => void;
    knowledgeDocumentsApi.list.mockResolvedValue({
      items: [sampleDocument, secondDocument],
      total: 2,
      offset: 0,
      limit: 20,
    });
    knowledgeDocumentsApi.detail
      .mockImplementationOnce(
        () =>
          new Promise<KnowledgeDocumentDetail>((resolve) => {
            resolveFirstDetail = resolve;
          }),
      )
      .mockResolvedValueOnce(documentDetail(secondDocument, "第二份历史"));
    renderPage();

    const detailButtons = await screen.findAllByRole("button", {
      name: "查看详情/历史",
    });
    await user.click(detailButtons[0]);
    await user.click(detailButtons[1]);
    expect(await screen.findByText("第二份历史")).toBeInTheDocument();

    resolveFirstDetail!(documentDetail(sampleDocument, "第一份历史"));
    await waitFor(() =>
      expect(screen.queryByText("第一份历史")).not.toBeInTheDocument(),
    );
  });

  it("关闭加载中的详情后可以继续打开另一份文档", async () => {
    const user = userEvent.setup();
    const secondDocument = {
      ...sampleDocument,
      id: 32,
      documentKey: "refund-guide",
      title: "退款说明",
      logicalPath: "docs/refunds.md",
      latestVersionId: 42,
    };
    let resolveFirstDetail: (detail: KnowledgeDocumentDetail) => void;
    knowledgeDocumentsApi.list.mockResolvedValue({
      items: [sampleDocument, secondDocument],
      total: 2,
      offset: 0,
      limit: 20,
    });
    knowledgeDocumentsApi.detail
      .mockImplementationOnce(
        () =>
          new Promise<KnowledgeDocumentDetail>((resolve) => {
            resolveFirstDetail = resolve;
          }),
      )
      .mockResolvedValueOnce(
        documentDetail(secondDocument, "恢复后的第二份历史"),
      );
    renderPage();

    const detailButtons = await screen.findAllByRole("button", {
      name: "查看详情/历史",
    });
    await user.click(detailButtons[0]);
    const detailDrawer = await findDrawer("文档详情与历史");
    await user.click(
      within(detailDrawer).getByRole("button", { name: "Close" }),
    );
    await user.click(detailButtons[1]);
    expect(await screen.findByText("恢复后的第二份历史")).toBeInTheDocument();

    resolveFirstDetail!(documentDetail(sampleDocument, "已关闭的第一份历史"));
    await waitFor(() =>
      expect(screen.queryByText("已关闭的第一份历史")).not.toBeInTheDocument(),
    );
  });

  it("回收站翻页后请求并显示对应页的数据", async () => {
    const user = userEvent.setup();
    knowledgeDocumentsApi.listDeleted.mockImplementation(
      async (input?: KnowledgeListInput) => {
        const offset = input?.offset ?? 0;
        return {
          items: [
            {
              ...sampleDocument,
              id: 100 + offset,
              title: `已删除文档 ${offset}`,
            },
          ],
          total: 21,
          offset,
          limit: 20,
        };
      },
    );
    renderPage();
    await screen.findByText("订单创建说明");

    await user.click(screen.getByRole("button", { name: "回收站" }));
    const recycleDrawer = await findDrawer("回收站");
    expect(
      await within(recycleDrawer).findByText("已删除文档 0"),
    ).toBeInTheDocument();
    await user.click(within(recycleDrawer).getByText("2", { exact: true }));
    await waitFor(() =>
      expect(knowledgeDocumentsApi.listDeleted).toHaveBeenLastCalledWith({
        projectId: 11,
        releaseId: null,
        keyword: null,
        offset: 20,
        limit: 20,
      }),
    );
    expect(
      await within(recycleDrawer).findByText("已删除文档 20"),
    ).toBeInTheDocument();
  });
});
