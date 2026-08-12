import { describe, expect, it } from "vitest";
import html from "../index.html?raw";
import startupSource from "./startup.ts?raw";
import tauriConfigSource from "../src-tauri/tauri.conf.json?raw";
import rustStartupSource from "../src-tauri/src/lib.rs?raw";

describe("应用启动静态契约", () => {
  it("在主脚本执行前提供可访问的启动状态", () => {
    const startupShellPosition = html.indexOf('class="startup-shell"');
    const startupScriptPosition = html.indexOf('src="/src/startup.ts"');

    expect(startupShellPosition).toBeGreaterThan(0);
    expect(startupShellPosition).toBeLessThan(startupScriptPosition);
    expect(html).toContain('role="status"');
    expect(html).toContain("正在启动应用…");
    expect(html).not.toContain('src="/src/main.tsx"');
    expect(startupSource).toContain('import("./main")');
    expect(startupSource).toContain("requestAnimationFrame");
    expect(startupSource).toContain("应用加载失败，请重新打开应用");
  });

  it("主窗口初始隐藏，并由首屏完成事件或超时兜底显示", () => {
    const config = JSON.parse(tauriConfigSource) as {
      app: { windows: Array<{ label?: string; visible?: boolean }> };
    };
    const mainWindow = config.app.windows.find(
      (window) => window.label === undefined || window.label === "main",
    );

    expect(mainWindow?.visible).toBe(false);
    expect(rustStartupSource).toContain("PageLoadEvent::Finished");
    expect(rustStartupSource).toContain("STARTUP_WINDOW_FALLBACK_TIMEOUT");
    expect(rustStartupSource).toContain("compare_exchange(false, true");
    expect(rustStartupSource).toContain("window.show()");
  });
});
