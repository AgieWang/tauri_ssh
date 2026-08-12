import { act, cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import DashboardPage from "./index";

const apiMocks = vi.hoisted(() => ({
  listServers: vi.fn(),
  listApprovals: vi.fn(),
  listProviders: vi.fn(),
  listAudits: vi.fn(),
  listJumpServers: vi.fn(),
  getMcpOverview: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  sshServerApi: { list: apiMocks.listServers },
  approvalApi: { list: apiMocks.listApprovals },
  aiProviderApi: { list: apiMocks.listProviders },
  auditApi: { list: apiMocks.listAudits },
  jumpserverApi: { list: apiMocks.listJumpServers },
  mcpApi: { overview: apiMocks.getMcpOverview },
  getErrorMessage: (error: unknown) => String(error),
}));

describe("DashboardPage 启动加载", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    Object.values(apiMocks).forEach((mock) => mock.mockReset());
    apiMocks.listServers.mockResolvedValue([]);
    apiMocks.listApprovals.mockResolvedValue([]);
    apiMocks.listProviders.mockResolvedValue([]);
    apiMocks.listAudits.mockResolvedValue([]);
    apiMocks.listJumpServers.mockResolvedValue([]);
    apiMocks.getMcpOverview.mockResolvedValue(null);
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("首屏立即读取核心数据，并在稳定后再读取审计与运行状态", async () => {
    render(
      <MemoryRouter>
        <DashboardPage />
      </MemoryRouter>,
    );

    expect(screen.getByText("工作台")).toBeVisible();
    expect(apiMocks.listServers).toHaveBeenCalledOnce();
    expect(apiMocks.listApprovals).toHaveBeenCalledOnce();
    expect(apiMocks.listProviders).toHaveBeenCalledOnce();
    expect(apiMocks.listAudits).not.toHaveBeenCalled();
    expect(apiMocks.listJumpServers).not.toHaveBeenCalled();
    expect(apiMocks.getMcpOverview).not.toHaveBeenCalled();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    expect(apiMocks.listAudits).toHaveBeenCalledOnce();
    expect(apiMocks.listJumpServers).toHaveBeenCalledOnce();
    expect(apiMocks.getMcpOverview).toHaveBeenCalledOnce();
  });
});
