import { useEffect, useState } from "react";
import { Card, Typography, message, Button, Space } from "antd";
import { SyncOutlined } from "@ant-design/icons";
import type { Update } from "@tauri-apps/plugin-updater";
import { hasTauriRuntime, systemApi, updaterApi } from "@/lib/api";
import { UpdateModal } from "@/components/ui/UpdateModal";
import packageJson from "../../../package.json";

const { Title, Text } = Typography;

export default function SettingsPage() {
  const [checking, setChecking] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  const [updateModalOpen, setUpdateModalOpen] = useState(false);
  const [appVersion, setAppVersion] = useState(packageJson.version);

  async function handleCheckUpdate() {
    if (!hasTauriRuntime()) {
      message.warning("浏览器预览不支持检查更新，请在桌面应用中使用该功能");
      return;
    }

    setChecking(true);
    try {
      const result = await updaterApi.checkUpdate();
      if (result) {
        setUpdate(result);
        setUpdateModalOpen(true);
      } else {
        message.success("当前已是最新版本");
      }
    } catch (e) {
      message.warning(`检查更新失败: ${String(e)}`);
    } finally {
      setChecking(false);
    }
  }

  useEffect(() => {
    systemApi
      .getSystemInfo()
      .then((info) => setAppVersion(info.appVersion || packageJson.version))
      // 浏览器调试没有 Tauri IPC，保留 Vite 注入的 package.json 版本作为兜底。
      .catch(() => {});
  }, []);

  return (
    <div className="max-w-2xl mx-auto">
      <Title level={3}>设置</Title>
      <Text type="secondary">应用配置管理（数据来自 Rust SQLite）</Text>

      <Card title="软件更新" className="mt-6">
        <Space>
          <Button
            icon={<SyncOutlined spin={checking} />}
            onClick={handleCheckUpdate}
            loading={checking}
          >
            检查更新
          </Button>
          <Text type="secondary">当前版本: {appVersion}</Text>
        </Space>
      </Card>

      <UpdateModal
        open={updateModalOpen}
        onClose={() => setUpdateModalOpen(false)}
        update={update}
      />
    </div>
  );
}
