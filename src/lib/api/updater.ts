import { check } from "@tauri-apps/plugin-updater";
import { hasTauriRuntime } from "./client";

/** 更新相关 API */
export const updaterApi = {
  checkUpdate: () => {
    if (!hasTauriRuntime()) {
      throw new Error("浏览器预览不支持检查更新，请在桌面应用中使用该功能");
    }
    return check();
  },
};
