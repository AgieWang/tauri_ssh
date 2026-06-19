import type { ThemeConfig } from "antd";
import { theme } from "antd";

/**
 * 暗色/亮色主题的共享 token
 * 与 variables.css 设计令牌保持同步
 */
const sharedToken: ThemeConfig["token"] = {
  borderRadius: 6,
  borderRadiusSM: 3,
  borderRadiusLG: 8,
  fontSize: 13,
  controlHeight: 32,
  controlHeightSM: 24,
  controlHeightLG: 40,
};

/** 共享的组件级覆盖 */
const sharedComponents: ThemeConfig["components"] = {
  Button: {
    borderRadius: 6,
  },
  Input: {
    borderRadius: 6,
  },
  Select: {
    borderRadius: 6,
  },
  Modal: {
    borderRadiusLG: 12,
  },
  Table: {
    borderRadius: 6,
  },
};

/** 暗色主题（与 variables.css :root[data-theme="dark"] 同步） */
export const darkTheme: ThemeConfig = {
  algorithm: theme.darkAlgorithm,
  token: {
    ...sharedToken,
    colorPrimary: "#5090f0",
    colorSuccess: "#5ec26a",
    colorWarning: "#e8c44a",
    colorError: "#f0554a",
    colorInfo: "#5090f0",
    colorBgContainer: "#232325",
    colorBgElevated: "#2a2a2d",
    colorBgLayout: "#1a1a1c",
    colorText: "#dcdcde",
    colorTextSecondary: "#8a8a8d",
    colorTextTertiary: "#707074",
    colorBorder: "#353538",
    colorBorderSecondary: "#2a2a2d",
  },
  components: {
    ...sharedComponents,
    Tooltip: {
      colorBgSpotlight: "#2a2a2d",
      colorTextLightSolid: "#dcdcde",
    },
  },
};

/** 亮色主题（与 variables.css :root[data-theme="light"] 同步） */
export const lightTheme: ThemeConfig = {
  algorithm: theme.defaultAlgorithm,
  token: {
    ...sharedToken,
    colorPrimary: "#2563eb",
    colorSuccess: "#16a34a",
    colorWarning: "#d97706",
    colorError: "#dc2626",
    colorInfo: "#2563eb",
    colorBgContainer: "#ffffff",
    colorBgElevated: "#ffffff",
    colorBgLayout: "#f5f5f7",
    colorText: "#1a1a2e",
    colorTextSecondary: "#5c5c6e",
    colorTextTertiary: "#9898a6",
    colorBorder: "#d9d9de",
    colorBorderSecondary: "#e8e8ec",
  },
  components: {
    ...sharedComponents,
    Tooltip: {
      colorBgSpotlight: "#1a1a2e",
      colorTextLightSolid: "#ffffff",
    },
  },
};

/** 根据 resolved theme 获取对应 Ant Design 主题配置 */
export function getAntdTheme(resolved: "light" | "dark"): ThemeConfig {
  return resolved === "dark" ? darkTheme : lightTheme;
}
