import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@/components/layout/AppLayout", async () => {
  const { createElement } = await import("react");
  const { Outlet } = await import("react-router-dom");
  return { AppLayout: () => createElement(Outlet) };
});

vi.mock("@/pages/knowledge/embedding/ProjectEmbeddingPage", async () => {
  const { createElement } = await import("react");
  return {
    default: () => createElement("main", null, "向量化独立页面"),
  };
});

describe("AppRouter", () => {
  afterEach(() => {
    cleanup();
    window.history.replaceState({}, "", "/");
  });

  it("将项目向量索引路径挂载到独立页面", async () => {
    window.history.replaceState({}, "", "/knowledge/projects/11/embedding");
    const { AppRouter } = await import("./Router");
    render(<AppRouter />);

    expect(await screen.findByText("向量化独立页面")).toBeVisible();
  });
});
