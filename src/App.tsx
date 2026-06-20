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

function App() {
  const appTheme = useAppStore((s) => s.theme);
  const setTheme = useAppStore((s) => s.setTheme);
  const [resolved, setResolved] = useState<"light" | "dark">(
    resolveTheme(appTheme)
  );

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

  return (
    <ConfigProvider locale={zhCN} theme={getAntdTheme(resolved)}>
      <ErrorBoundary>
        <AppRouter />
        <StartupUpdateChecker />
      </ErrorBoundary>
    </ConfigProvider>
  );
}

export default App;
