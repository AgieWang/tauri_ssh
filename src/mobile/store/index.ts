// 移动端 Zustand store 骨架（占位，按业务自行扩展）
//
// 与桌面端 src/store/ 完全独立。

import { create } from "zustand";

interface MobileAppStore {
  /** 当前移动端 UI 主题 */
  theme: "light" | "dark";
  setTheme: (t: "light" | "dark") => void;
}

export const useMobileAppStore = create<MobileAppStore>((set) => ({
  theme: "light",
  setTheme: (theme) => set({ theme }),
}));
