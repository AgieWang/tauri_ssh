import { create } from "zustand";

interface SettingsStore {
  /** 语言 */
  language: string;
  /** 设置语言 */
  setLanguage: (lang: string) => void;
  /** 关闭行为：minimize（最小化到托盘）| exit（直接退出） */
  closeBehavior: "minimize" | "exit";
  /** 设置关闭行为 */
  setCloseBehavior: (behavior: "minimize" | "exit") => void;
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  language: "zh-CN",
  setLanguage: (language) => set({ language }),
  closeBehavior: "minimize",
  setCloseBehavior: (closeBehavior) => set({ closeBehavior }),
}));
