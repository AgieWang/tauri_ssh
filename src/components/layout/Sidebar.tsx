import { useNavigate, useLocation } from "react-router-dom";
import { Menu } from "antd";
import {
  Bot,
  BookOpen,
  Activity,
  Database,
  Clock3,
  FolderTree,
  GitBranch,
  KeyRound,
  Landmark,
  LayoutDashboard,
  LockKeyhole,
  Logs,
  PlugZap,
  Rocket,
  ScrollText,
  Server,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  Terminal,
} from "lucide-react";
import { useAppStore } from "@/store";

const menuItems = [
  {
    key: "/dashboard",
    icon: <LayoutDashboard size={18} />,
    label: "工作台",
  },
  {
    key: "assets",
    icon: <Server size={18} />,
    label: "资产",
    children: [
      { key: "/servers", icon: <Server size={16} />, label: "服务器" },
      { key: "/vault", icon: <KeyRound size={16} />, label: "凭据保险库" },
    ],
  },
  {
    key: "secure-credentials",
    icon: <LockKeyhole size={18} />,
    label: "安全凭证",
    children: [
      { key: "/secure-credentials/overview", icon: <LayoutDashboard size={16} />, label: "概览" },
      { key: "/secure-credentials/vault", icon: <KeyRound size={16} />, label: "凭证库" },
      { key: "/secure-credentials/git-workspaces", icon: <GitBranch size={16} />, label: "Git 工作区" },
      { key: "/secure-credentials/sessions", icon: <Clock3 size={16} />, label: "会话" },
      { key: "/secure-credentials/mcp", icon: <PlugZap size={16} />, label: "MCP 接入" },
      { key: "/secure-credentials/audit", icon: <ScrollText size={16} />, label: "审计" },
      { key: "/secure-credentials/policies", icon: <SlidersHorizontal size={16} />, label: "策略" },
    ],
  },
  {
    key: "ops",
    icon: <Terminal size={18} />,
    label: "运维",
    children: [
      { key: "/terminal", icon: <Terminal size={16} />, label: "终端 + AI" },
      { key: "/logs", icon: <Logs size={16} />, label: "日志监听" },
      { key: "/sftp", icon: <FolderTree size={16} />, label: "SFTP 文件" },
      { key: "/database", icon: <Database size={16} />, label: "数据库管理" },
      { key: "/resource-monitor", icon: <Activity size={16} />, label: "资源监控" },
      { key: "/deployments", icon: <Rocket size={16} />, label: "自动部署" },
    ],
  },
  {
    key: "ai",
    icon: <Bot size={18} />,
    label: "AI / MCP",
    children: [
      { key: "/providers", icon: <Bot size={16} />, label: "AI Provider" },
      { key: "/mcp", icon: <PlugZap size={16} />, label: "MCP Server" },
      { key: "/skills", icon: <BookOpen size={16} />, label: "Skill 管理" },
      { key: "/jumpserver", icon: <Landmark size={16} />, label: "堡垒机会话" },
    ],
  },
  {
    key: "governance",
    icon: <ScrollText size={18} />,
    label: "治理",
    children: [
      { key: "/audit", icon: <ScrollText size={16} />, label: "审计日志" },
      { key: "/approval", icon: <ShieldCheck size={16} />, label: "审批队列" },
      { key: "/prototype-settings", icon: <Settings size={16} />, label: "系统设置" },
    ],
  },
];

export function Sidebar() {
  const navigate = useNavigate();
  const location = useLocation();
  const collapsed = useAppStore((s) => s.sidebarCollapsed);

  return (
    <div className="flex flex-col h-full">
      <div
        className="h-12 flex items-center justify-center font-bold text-base"
        style={{
          borderBottom: `1px solid var(--border)`,
          color: "var(--text-primary)",
          background: "var(--bg-secondary)",
        }}
      >
        {collapsed ? "SSH" : "Tauri SSH"}
      </div>
      <div
        className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden"
        style={{ scrollbarGutter: "stable" }}
      >
        <Menu
          mode="inline"
          selectedKeys={[location.pathname === "/" ? "/dashboard" : location.pathname]}
          defaultOpenKeys={collapsed ? [] : ["assets", "secure-credentials", "ops", "ai", "governance"]}
          items={menuItems}
          onClick={({ key }) => {
            if (String(key).startsWith("/")) {
              navigate(key);
            }
          }}
          style={{ border: "none" }}
        />
      </div>
    </div>
  );
}
