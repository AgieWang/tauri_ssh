import { useEffect, useState } from "react";
import { message } from "antd";
import type { Update } from "@tauri-apps/plugin-updater";
import { hasTauriRuntime, updaterApi } from "@/lib/api";
import { UpdateModal } from "@/components/ui/UpdateModal";

let startupUpdateChecked = false;

export function StartupUpdateChecker() {
  const [update, setUpdate] = useState<Update | null>(null);

  useEffect(() => {
    if (startupUpdateChecked || !hasTauriRuntime()) {
      return;
    }
    startupUpdateChecked = true;

    updaterApi
      .checkUpdate()
      .then((result) => {
        if (result) {
          setUpdate(result);
        }
      })
      .catch((error) => {
        message.warning(`自动检查更新失败: ${String(error)}`);
      });
  }, []);

  return (
    <UpdateModal
      open={Boolean(update)}
      onClose={() => undefined}
      update={update}
      forceUpdate
    />
  );
}
