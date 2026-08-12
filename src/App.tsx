import { useEffect, useState } from "react";
import { ConfigProvider } from "antd";
import zhCN from "antd/locale/zh_CN";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";
import { useAppStore } from "@/store";
import { resolveTheme } from "@/store/app";
import { getAntdTheme } from "@/theme/antdTheme";
import { AppRouter } from "@/Router";
import { systemSettingsApi } from "@/lib/api";
import { StartupUpdateChecker } from "@/components/ui/StartupUpdateChecker";
import { StartupAutoLaunchPrompt } from "@/components/ui/StartupAutoLaunchPrompt";

function App() {
  const appTheme = useAppStore((s) => s.theme);
  const setTheme = useAppStore((s) => s.setTheme);
  const [resolved, setResolved] = useState<"light" | "dark">(
    resolveTheme(appTheme),
  );
  const [startupTasksReady, setStartupTasksReady] = useState(false);

  useEffect(() => {
    let mounted = true;
    systemSettingsApi
      .get()
      .then((settings) => {
        if (mounted) {
          setTheme(settings.theme);
        }
      })
      .catch(() => undefined);
    return () => {
      mounted = false;
    };
  }, [setTheme]);

  useEffect(() => {
    // 非 system 模式直接应用
    if (appTheme !== "system") {
      setResolved(appTheme);
      return;
    }

    // system 模式：立即解析 + 监听系统变化
    setResolved(resolveTheme("system"));
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (e: MediaQueryListEvent) =>
      setResolved(e.matches ? "dark" : "light");
    mql.addEventListener("change", handler);
    return () => mql.removeEventListener("change", handler);
  }, [appTheme]);

  // 将 resolved theme 写入 DOM，驱动 CSS 变量切换
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", resolved);
  }, [resolved]);

  useEffect(() => {
    const applyInputTextBehavior = () => {
      document
        .querySelectorAll<HTMLInputElement | HTMLTextAreaElement>(
          "input, textarea",
        )
        .forEach((element) => {
          element.setAttribute("autocapitalize", "none");
          element.setAttribute("autocorrect", "off");
          element.setAttribute("autocomplete", "off");
          element.spellcheck = false;
        });
    };

    applyInputTextBehavior();
    const observer = new MutationObserver(applyInputTextBehavior);
    observer.observe(document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    // 更新检查和一次性提示不参与首屏展示，延后挂载以免与工作台数据争用 IPC。
    const timer = window.setTimeout(() => setStartupTasksReady(true), 1_500);
    return () => window.clearTimeout(timer);
  }, []);

  return (
    <ConfigProvider locale={zhCN} theme={getAntdTheme(resolved)}>
      <ErrorBoundary>
        <AppRouter />
        {startupTasksReady ? (
          <>
            <StartupAutoLaunchPrompt />
            <StartupUpdateChecker />
          </>
        ) : null}
      </ErrorBoundary>
    </ConfigProvider>
  );
}

export default App;
