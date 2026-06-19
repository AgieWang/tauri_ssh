import { useRef, type ReactNode } from "react";
import { Outlet, useNavigate } from "react-router-dom";
import { Layout, Button, theme as antdTheme } from "antd";
import { MenuFoldOutlined, MenuUnfoldOutlined, SettingOutlined } from "@ant-design/icons";
import { getCurrentWindow, type Window } from "@tauri-apps/api/window";
import { useAppStore } from "@/store";
import { Sidebar } from "./Sidebar";
import { WindowControls } from "./WindowControls";
import { ThemeToggle } from "@/components/ui/ThemeToggle";

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
          <div style={{ display: "flex", alignItems: "center" }}>
            {headerExtra}
            <ThemeToggle />
            <Button
              type="text"
              icon={<SettingOutlined />}
              onClick={() => navigate("/settings")}
              title="设置"
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
