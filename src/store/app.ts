import { create } from "zustand";

export type ThemeMode = "light" | "dark" | "system";

/** 解析实际生效的主题（system → 读取系统偏好） */
export function resolveTheme(mode: ThemeMode): "light" | "dark" {
  if (mode !== "system") return mode;
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

interface AppStore {
  /** 主题模式（含跟随系统） */
  theme: ThemeMode;
  /** 侧边栏是否折叠 */
  sidebarCollapsed: boolean;
  /** 三态切换主题：dark → light → system → dark */
  toggleTheme: () => void;
  /** 设置主题 */
  setTheme: (theme: ThemeMode) => void;
  /** 切换侧边栏 */
  toggleSidebar: () => void;
}

const TOGGLE_ORDER: ThemeMode[] = ["dark", "light", "system"];

export const useAppStore = create<AppStore>((set) => ({
  theme: "system",
  sidebarCollapsed: false,
  toggleTheme: () =>
    set((s) => {
      const idx = TOGGLE_ORDER.indexOf(s.theme);
      const next = TOGGLE_ORDER[(idx + 1) % TOGGLE_ORDER.length];
      return { theme: next };
    }),
  setTheme: (theme) => set({ theme }),
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
}));
