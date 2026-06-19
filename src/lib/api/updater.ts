import { check } from "@tauri-apps/plugin-updater";

/** 更新相关 API */
export const updaterApi = {
  checkUpdate: () => check(),
};
