import { useEffect } from "react";
import { Modal, message } from "antd";
import {
  configApi,
  getErrorMessage,
  hasTauriRuntime,
  systemSettingsApi,
} from "@/lib/api";

const AUTO_LAUNCH_PROMPTED_KEY = "settings.launch_on_startup_prompted";

let startupAutoLaunchChecked = false;

async function hasPromptedAutoLaunch() {
  try {
    return (await configApi.get(AUTO_LAUNCH_PROMPTED_KEY)) === "true";
  } catch {
    return false;
  }
}

async function markPrompted() {
  await configApi.set(AUTO_LAUNCH_PROMPTED_KEY, "true");
}

export function StartupAutoLaunchPrompt() {
  useEffect(() => {
    if (startupAutoLaunchChecked || !hasTauriRuntime()) {
      return;
    }
    startupAutoLaunchChecked = true;

    async function checkAutoLaunchPrompt() {
      try {
        if (await hasPromptedAutoLaunch()) {
          return;
        }

        const settings = await systemSettingsApi.get();
        if (settings.launchOnStartup) {
          await markPrompted();
          return;
        }

        Modal.confirm({
          title: "是否开启开机自启动？",
          content:
            "开启后，Tauri SSH 会在系统登录后自动启动，便于接收运维提醒和快速进入工作台。",
          okText: "开启",
          cancelText: "暂不开启",
          async onOk() {
            const next = { ...settings, launchOnStartup: true };
            await systemSettingsApi.update(next);
            await markPrompted();
            message.success("已开启开机自启动");
          },
          async onCancel() {
            await markPrompted();
          },
        });
      } catch (error) {
        message.warning(`开机自启动检测失败: ${getErrorMessage(error)}`);
      }
    }

    void checkAutoLaunchPrompt();
  }, []);

  return null;
}
