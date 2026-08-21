import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { MarkdownPreview } from "./MarkdownPreview";

describe("MarkdownPreview", () => {
  afterEach(() => cleanup());

  it("将 AI 分析代码引用渲染为可读证据标签而不是内部引用键", () => {
    render(
      <MarkdownPreview content="订单流程由服务层处理。[code:1:file:291][code:1:file:292]" />,
    );

    expect(
      screen.getByLabelText("代码证据：快照 1，文件 291"),
    ).toHaveTextContent("代码证据 · 文件 #291");
    expect(
      screen.getByLabelText("代码证据：快照 1，文件 292"),
    ).toHaveTextContent("代码证据 · 文件 #292");
    expect(screen.queryByText("[code:1:file:291]")).not.toBeInTheDocument();
  });

  it("将代码快照片段引用渲染为可读证据标签而不是内部键", () => {
    render(
      <MarkdownPreview content="同步状态已更新。[code:snapshot:2:chunk:2641]" />,
    );

    expect(
      screen.getByLabelText("代码证据：快照 2，片段 2641"),
    ).toHaveTextContent("代码证据 · 片段 #2641");
    expect(
      screen.queryByText("[code:snapshot:2:chunk:2641]"),
    ).not.toBeInTheDocument();
  });

  it("将模型返回的证据片段引用渲染为可读标签而不是内部键", () => {
    render(<MarkdownPreview content="生成入口见 [citation:chunk:885]。" />);

    const citation = screen.getByLabelText("证据片段 885");
    expect(citation).toHaveTextContent("证据片段 · 片段 #885");
    expect(citation).toHaveClass(
      "bg-[var(--bg-tertiary)]",
      "text-[var(--accent)]",
    );
    expect(screen.queryByText("[citation:chunk:885]")).not.toBeInTheDocument();
  });

  it("兼容模型返回的简写证据片段引用", () => {
    render(<MarkdownPreview content="生成规则见 [citation:885]。" />);

    expect(screen.getByLabelText("证据片段 885")).toHaveTextContent(
      "证据片段 · 片段 #885",
    );
    expect(screen.queryByText("[citation:885]")).not.toBeInTheDocument();
  });

  it("兼容模型返回的带 citation 前缀的完整代码引用", () => {
    render(
      <MarkdownPreview content="候选 SQL 见 [citation:code:snapshot:2:chunk:8051]。" />,
    );

    expect(
      screen.getByLabelText("代码证据：快照 2，片段 8051"),
    ).toHaveTextContent("代码证据 · 片段 #8051");
    expect(
      screen.queryByText("[citation:code:snapshot:2:chunk:8051]"),
    ).not.toBeInTheDocument();
  });

  it("将带完整文档引用键的模型引用转换为证据标签", () => {
    render(
      <MarkdownPreview content="规则见 [citation:document:1:version:1:chunk:1]。" />,
    );

    expect(screen.getByLabelText("证据片段 1")).toHaveTextContent(
      "证据片段 · 片段 #1",
    );
    expect(
      screen.queryByText("[citation:document:1:version:1:chunk:1]"),
    ).not.toBeInTheDocument();
  });

  it("将模型原样输出的裸文档引用键转换为证据标签", () => {
    render(
      <MarkdownPreview content="规则见 [document:1:version:1:chunk:1]。" />,
    );

    expect(screen.getByLabelText("证据片段 1")).toHaveTextContent(
      "证据片段 · 片段 #1",
    );
    expect(
      screen.queryByText("[document:1:version:1:chunk:1]"),
    ).not.toBeInTheDocument();
  });

  it("将 Git 工具引用渲染为可读标签而不是内部键", () => {
    render(
      <MarkdownPreview content="提交数为 249。[tool:git_commit_count:release:1:repository:2]" />,
    );

    expect(screen.getByLabelText("Git 实时证据")).toHaveTextContent(
      "Git 实时证据",
    );
    expect(
      screen.queryByText("[tool:git_commit_count:release:1:repository:2]"),
    ).not.toBeInTheDocument();
  });

  it("使用语义主题令牌渲染正文和内联元素", () => {
    render(
      <MarkdownPreview
        content={
          "## 需求说明\n\n详见 **核心规则**、`OrderService` 与 [接口文档](https://example.com)。"
        }
      />,
    );

    expect(screen.getByRole("article")).toHaveClass(
      "text-[var(--text-primary)]",
    );
    expect(screen.getByRole("heading", { name: "需求说明" })).toHaveClass(
      "text-[var(--text-primary)]",
    );
    expect(screen.getByText("核心规则")).toHaveClass(
      "text-[var(--text-primary)]",
    );
    expect(screen.getByText("OrderService")).toHaveClass(
      "bg-[var(--bg-tertiary)]",
      "text-[var(--text-primary)]",
    );
    expect(screen.getByRole("link", { name: "接口文档" })).toHaveClass(
      "text-[var(--accent)]",
    );
  });

  it("为多列表格保留可读列宽并在内容区域内横向滚动", () => {
    render(
      <MarkdownPreview
        content={
          "| 需求 | 判断 | 实现证据 |\n|---|---|---|\n| 一键删除 | 待确认 | 很长的代码与测试证据内容 |"
        }
      />,
    );

    const table = screen.getByRole("table");
    expect(table).toHaveClass("min-w-[720px]", "table-auto");
    expect(table.parentElement).toHaveClass("max-w-full", "overflow-x-auto");
    expect(screen.getByRole("columnheader", { name: "需求" })).toHaveClass(
      "min-w-32",
      "break-words",
    );
    expect(screen.getByRole("columnheader", { name: "判断" })).toHaveClass(
      "min-w-32",
    );
    expect(screen.getByRole("columnheader", { name: "实现证据" })).toHaveClass(
      "min-w-64",
    );
    expect(screen.getByRole("cell", { name: "一键删除" })).toHaveClass(
      "min-w-32",
      "break-words",
    );
    expect(
      screen.getByRole("cell", { name: "很长的代码与测试证据内容" }),
    ).toHaveClass("min-w-64");
  });
});
