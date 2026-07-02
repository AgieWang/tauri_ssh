import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Outlet, useNavigate } from "react-router-dom";
import { Layout, Button, Modal, Tooltip, message, theme as antdTheme } from "antd";
import { MenuFoldOutlined, MenuUnfoldOutlined, SettingOutlined, SyncOutlined } from "@ant-design/icons";
import { getCurrentWindow, type Window } from "@tauri-apps/api/window";
import type { Update } from "@tauri-apps/plugin-updater";
import { ShieldAlert } from "lucide-react";
import { useAppStore } from "@/store";
import { Sidebar } from "./Sidebar";
import { WindowControls } from "./WindowControls";
import { ThemeToggle } from "@/components/ui/ThemeToggle";
import { UpdateModal } from "@/components/ui/UpdateModal";
import { getErrorMessage, hasTauriRuntime, systemApi, systemSettingsApi, updaterApi } from "@/lib/api";
import type { AiUnrestrictedState } from "@/types";
import packageJson from "../../../package.json";

const { Header, Sider, Content } = Layout;

function getAppWindow(): Window | null {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

/** Header 中间的可拖拽空白区域 */
function DragRegion() {
  const windowRef = useRef<Window | null>(getAppWindow());

  function handleMouseDown(e: React.MouseEvent) {
    if (e.buttons === 1 && windowRef.current) {
      if (e.detail === 2) {
        windowRef.current.toggleMaximize();
      } else {
        windowRef.current.startDragging();
      }
    }
  }

  return (
    <div
      onMouseDown={handleMouseDown}
      style={{
        flex: 1,
        height: "100%",
        cursor: "default",
        userSelect: "none",
      }}
    />
  );
}

function formatRemaining(seconds: number) {
  if (seconds <= 0) return "AI 放行";
  const minutes = Math.ceil(seconds / 60);
  return `${minutes} 分钟`;
}

function AiUnrestrictedButton() {
  const [state, setState] = useState<AiUnrestrictedState>({
    active: false,
    until: null,
    remainingSeconds: 0,
  });
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setState(await systemSettingsApi.getAiUnrestrictedState());
    } catch {
      setState({ active: false, until: null, remainingSeconds: 0 });
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 30_000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const enable = () => {
    Modal.confirm({
      title: "开启 AI 临时放行？",
      okText: "开启 30 分钟",
      cancelText: "取消",
      okButtonProps: { danger: true },
      content: (
        <div>
          <p>开启后 30 分钟内，AI 可自动执行读写命令，并跳过 AI 审计记录。</p>
          <p>系统设置中的危险命令黑名单仍会在本地强制阻止，不能绕过。</p>
        </div>
      ),
      async onOk() {
        setLoading(true);
        try {
          const next = await systemSettingsApi.enableAiUnrestrictedMode({ minutes: 30 });
          setState(next);
          message.warning("AI 临时放行已开启 30 分钟");
        } catch (error) {
          message.error(getErrorMessage(error));
        } finally {
          setLoading(false);
        }
      },
    });
  };

  const disable = async () => {
    setLoading(true);
    try {
      const next = await systemSettingsApi.disableAiUnrestrictedMode();
      setState(next);
      message.success("AI 临时放行已关闭");
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  };

  return (
    <Tooltip title={state.active ? "点击关闭 AI 临时放行" : "30 分钟内允许 AI 自动执行读写命令，危险命令仍阻止"}>
      <Button
        danger={state.active}
        type={state.active ? "primary" : "default"}
        size="small"
        className={`header-pill-toggle ai-unrestricted-toggle ${state.active ? "ai-unrestricted-toggle-active" : ""}`}
        icon={<ShieldAlert size={15} />}
        loading={loading}
        onClick={state.active ? () => void disable() : enable}
      >
        <span className="header-pill-toggle-dot" />
        <span>{state.active ? formatRemaining(state.remainingSeconds) : "AI 放行"}</span>
        <span className="header-pill-toggle-state">{state.active ? "开" : "关"}</span>
      </Button>
    </Tooltip>
  );
}

function HeaderUpdateButton() {
  const [checking, setChecking] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  const [updateModalOpen, setUpdateModalOpen] = useState(false);
  const [appVersion, setAppVersion] = useState(packageJson.version);
  const versionLabel = `v${appVersion}${import.meta.env.DEV ? " DEV" : ""}`;

  useEffect(() => {
    systemApi
      .getSystemInfo()
      .then((info) => setAppVersion(info.appVersion || packageJson.version))
      // 浏览器调试没有 Tauri IPC，保留 package.json 版本作为兜底。
      .catch(() => {});
  }, []);

  async function checkUpdate() {
    if (!hasTauriRuntime()) {
      message.warning("浏览器预览不支持检查更新，请在桌面应用中使用该功能");
      return;
    }
    setChecking(true);
    try {
      const result = await updaterApi.checkUpdate();
      if (result) {
        setUpdate(result);
        setUpdateModalOpen(true);
      } else {
        message.success("当前已是最新版本");
      }
    } catch (error) {
      message.warning(`检查更新失败: ${String(error)}`);
    } finally {
      setChecking(false);
    }
  }

  return (
    <>
      <Tooltip title={`当前版本：${versionLabel}，点击检查更新`}>
        <Button
          size="small"
          className="header-pill-toggle header-update-toggle"
          icon={<SyncOutlined spin={checking} />}
          loading={checking}
          onClick={() => void checkUpdate()}
        >
          <span className="header-pill-toggle-dot header-update-toggle-dot" />
          <span>{versionLabel}</span>
        </Button>
      </Tooltip>
      <UpdateModal
        open={updateModalOpen}
        onClose={() => setUpdateModalOpen(false)}
        update={update}
      />
    </>
  );
}

interface AppLayoutProps {
  /** Header 右侧额外操作区（插入在主题切换和设置按钮之前） */
  headerExtra?: ReactNode;
}

export function AppLayout({ headerExtra }: AppLayoutProps) {
  const { sidebarCollapsed, toggleSidebar } = useAppStore();
  const { token } = antdTheme.useToken();
  const navigate = useNavigate();

  return (
    <Layout style={{ height: "100vh" }}>
      <Sider
        collapsed={sidebarCollapsed}
        collapsedWidth={60}
        width={220}
        style={{
          background: token.colorBgContainer,
          borderRight: `1px solid var(--border)`,
        }}
      >
        <Sidebar />
      </Sider>
      <Layout>
        <Header
          style={{
            padding: 0,
            height: "var(--header-height)",
            lineHeight: "var(--header-height)",
            display: "flex",
            alignItems: "center",
            background: token.colorBgContainer,
            borderBottom: `1px solid var(--border)`,
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 4, paddingLeft: 16 }}>
            <Button
              type="text"
              icon={
                sidebarCollapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />
              }
              onClick={toggleSidebar}
            />
          </div>
          <DragRegion />
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            {headerExtra}
            <HeaderUpdateButton />
            <AiUnrestrictedButton />
            <ThemeToggle />
            <Button
              type="text"
              icon={<SettingOutlined />}
              onClick={() => navigate("/prototype-settings")}
              title="系统设置"
            />
            <WindowControls />
          </div>
        </Header>
        <Content
          style={{
            padding: 24,
            overflow: "auto",
            background: token.colorBgLayout,
          }}
        >
          <Outlet />
        </Content>
      </Layout>
    </Layout>
  );
}
