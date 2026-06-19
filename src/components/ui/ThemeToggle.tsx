import { Button, Tooltip } from "antd";
import { Sun, Moon, Monitor } from "lucide-react";
import { useAppStore } from "@/store";

const THEME_CONFIG = {
  dark: { icon: <Moon size={16} />, label: "暗色主题" },
  light: { icon: <Sun size={16} />, label: "亮色主题" },
  system: { icon: <Monitor size={16} />, label: "跟随系统" },
} as const;

/** 三态主题切换按钮：dark → light → system → dark */
export function ThemeToggle() {
  const { theme, toggleTheme } = useAppStore();
  const config = THEME_CONFIG[theme];

  return (
    <Tooltip title={config.label}>
      <Button type="text" icon={config.icon} onClick={toggleTheme} />
    </Tooltip>
  );
}
