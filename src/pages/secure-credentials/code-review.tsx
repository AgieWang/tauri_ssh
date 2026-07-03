import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Alert,
  Button,
  Card,
  Descriptions,
  Drawer,
  Empty,
  Form,
  Input,
  Modal,
  Select,
  Space,
  Table,
  Tabs,
  Tag,
  Tooltip,
  Typography,
  message,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import {
  Bot,
  Ban,
  ClipboardCopy,
  Eye,
  GitBranch,
  GitMerge,
  GitPullRequestArrow,
  RefreshCw,
  Search,
  ShieldCheck,
  Undo2,
  Upload,
} from "lucide-react";
import type { Key } from "react";
import { codeReviewApi, getErrorMessage, gitWorkspaceApi } from "@/lib/api";
import type {
  CodeReviewBatchItem,
  CodeReviewChangedFile,
  CodeReviewTask,
  GitWorkspace,
  GitWorkspaceBranch,
} from "@/types";
import { PageHeader } from "@/components/prototype/common";

const { Text } = Typography;

interface BatchDraftItem extends CodeReviewBatchItem {
  selectedWorkspaceKey?: string;
}

interface DiffExcerpt {
  path: string;
  content: string;
  truncated?: boolean;
}

interface BatchRunSummary {
  success: number;
  failed: number;
  conflict: number;
  stale: number;
  skipped: number;
  pending: number;
  errors: string[];
}

const riskColor: Record<string, string> = {
  unknown: "default",
  low: "green",
  medium: "gold",
  high: "orange",
  critical: "red",
};

const riskText: Record<string, string> = {
  unknown: "未评估",
  low: "低风险",
  medium: "中风险",
  high: "高风险",
  critical: "严重风险",
};

const statusText: Record<string, string> = {
  draft: "草稿",
  diff_ready: "Diff 已生成",
  reviewing: "审查中",
  review_ready: "审查完成",
  merge_pending: "待合并",
  merged: "本地已合并",
  merge_failed: "合并失败",
  conflict: "冲突",
  stale: "已过期",
  cancelled: "已取消",
};

const pushText: Record<string, string> = {
  not_requested: "远程未推送",
  pushing: "推送中",
  pushed: "已推送",
  push_failed: "推送失败",
};

const batchStatusText: Record<string, string> = {
  matched: "已匹配",
  unmatched: "未匹配",
  low_confidence: "置信度不足",
};

export default function SecureCredentialCodeReviewPage() {
  const [workspaces, setWorkspaces] = useState<GitWorkspace[]>([]);
  const [branches, setBranches] = useState<GitWorkspaceBranch[]>([]);
  const [tasks, setTasks] = useState<CodeReviewTask[]>([]);
  const [activeTask, setActiveTask] = useState<CodeReviewTask | null>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [batchText, setBatchText] = useState("");
  const [batchKey, setBatchKey] = useState("");
  const [batchItems, setBatchItems] = useState<BatchDraftItem[]>([]);
  const [batchSummary, setBatchSummary] = useState<BatchRunSummary | null>(null);
  const [branchCache, setBranchCache] = useState<Record<string, GitWorkspaceBranch[]>>({});
  const [selectedTaskKeys, setSelectedTaskKeys] = useState<Key[]>([]);
  const [reviewingTaskKeys, setReviewingTaskKeys] = useState<Set<string>>(new Set());
  const reviewingTaskKeysRef = useRef<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [form] = Form.useForm();

  const selectedWorkspaceKey = Form.useWatch("workspaceKey", form);
  const selectedSourceBranch = Form.useWatch("sourceBranch", form);
  const selectedTargetBranch = Form.useWatch("targetBranch", form);

  const loadWorkspaces = useCallback(async () => {
    try {
      const rows = await gitWorkspaceApi.list();
      setWorkspaces(rows);
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }, []);

  const loadTasks = useCallback(async () => {
    try {
      const rows = await codeReviewApi.list({ limit: 100 });
      setTasks(rows);
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }, []);

  const loadBranches = useCallback(async (workspaceKey: string) => {
    if (!workspaceKey) {
      setBranches([]);
      return;
    }
    try {
      const rows = await gitWorkspaceApi.branches(workspaceKey);
      setBranches(rows);
    } catch (error) {
      setBranches([]);
      message.error(getErrorMessage(error));
    }
  }, []);

  const loadWorkspaceBranchesCached = useCallback(
    async (workspaceKey: string) => {
      if (!workspaceKey || branchCache[workspaceKey]) {
        return;
      }
      try {
        const rows = await gitWorkspaceApi.branches(workspaceKey);
        setBranchCache((cache) => ({ ...cache, [workspaceKey]: rows }));
      } catch (error) {
        message.warning(`读取工作区分支失败：${getErrorMessage(error)}`);
      }
    },
    [branchCache],
  );

  useEffect(() => {
    void loadWorkspaces();
    void loadTasks();
  }, [loadTasks, loadWorkspaces]);

  useEffect(() => {
    void loadBranches(selectedWorkspaceKey);
  }, [loadBranches, selectedWorkspaceKey]);

  const workspaceOptions = useMemo(
    () =>
      workspaces.map((workspace) => ({
        label: `${workspace.name} (${workspace.branch || "unknown"})`,
        value: workspace.workspaceKey,
      })),
    [workspaces],
  );

  const branchOptions = useMemo(
    () =>
      branches.map((branch) => ({
        label: branch.displayName,
        value: branch.name,
      })),
    [branches],
  );

  const selectedWorkspace = useMemo(
    () => workspaces.find((workspace) => workspace.workspaceKey === selectedWorkspaceKey),
    [selectedWorkspaceKey, workspaces],
  );

  const selectedSourceBranchInfo = useMemo(
    () => branches.find((branch) => branch.name === selectedSourceBranch),
    [branches, selectedSourceBranch],
  );

  const selectedTargetBranchInfo = useMemo(
    () => branches.find((branch) => branch.name === selectedTargetBranch),
    [branches, selectedTargetBranch],
  );

  const supersededTaskMap = useMemo(() => {
    const latestCompletedByGroup = new Map<string, CodeReviewTask>();
    for (const task of tasks) {
      if (task.status !== "merged" && task.pushStatus !== "pushed") {
        continue;
      }
      const key = reviewGroupKey(task);
      const current = latestCompletedByGroup.get(key);
      if (!current || task.id > current.id) {
        latestCompletedByGroup.set(key, task);
      }
    }

    const map: Record<string, { task: CodeReviewTask; reason: string }> = {};
    for (const task of tasks) {
      const latestCompleted = latestCompletedByGroup.get(reviewGroupKey(task));
      if (!latestCompleted || latestCompleted.taskKey === task.taskKey || task.id >= latestCompleted.id) {
        continue;
      }
      if (task.status === "merged" || task.status === "stale" || task.status === "cancelled" || task.pushStatus === "pushed") {
        continue;
      }
      map[task.taskKey] = {
        task: latestCompleted,
        reason:
          latestCompleted.pushStatus === "pushed"
            ? "已有更新的同分支任务完成远程推送"
            : "已有更新的同分支任务完成本地合并",
      };
    }
    return map;
  }, [tasks]);

  function getTaskSupersededInfo(task: CodeReviewTask) {
    return supersededTaskMap[task.taskKey];
  }

  function isTaskSuperseded(task: CodeReviewTask) {
    return Boolean(getTaskSupersededInfo(task));
  }

  function getCachedBranch(workspaceKey: string | undefined, branchName: string) {
    if (!workspaceKey || !branchName) {
      return undefined;
    }
    return branchCache[workspaceKey]?.find((branch) => branch.name === branchName);
  }

  function branchLastCommitText(workspaceKey: string | undefined, branchName: string) {
    const branch = getCachedBranch(workspaceKey, branchName);
    if (!branch) {
      return "-";
    }
    return `${branch.lastCommitHash || "-"} ${branch.lastCommitMessage || ""}`.trim();
  }

  function batchBranchOptions(workspaceKey: string | undefined, currentValue: string) {
    const rows = workspaceKey ? branchCache[workspaceKey] ?? [] : [];
    const options = rows.map((branch) => ({
      label: branch.displayName || branch.name,
      value: branch.name,
    }));
    if (currentValue && !options.some((option) => option.value === currentValue)) {
      return [{ label: currentValue, value: currentValue }, ...options];
    }
    return options;
  }

  function isHighRiskTask(task: CodeReviewTask) {
    return isHighRiskReview(task.riskLevel, task.targetBranch);
  }

  async function createAndPrepare() {
    const values = await form.validateFields();
    setLoading(true);
    try {
      const created = await codeReviewApi.create(values);
      const prepared = await codeReviewApi.prepareDiff(created.taskKey);
      setActiveTask(prepared);
      setDetailOpen(true);
      await loadTasks();
      message.success("Diff 已生成");
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }

  async function runAi(task: CodeReviewTask) {
    const superseded = getTaskSupersededInfo(task);
    if (superseded) {
      message.warning(`${superseded.reason}，请查看最新任务 ${superseded.task.taskKey}`);
      return;
    }
    if (reviewingTaskKeysRef.current.has(task.taskKey)) {
      return;
    }
    reviewingTaskKeysRef.current.add(task.taskKey);
    setReviewingTaskKeys((keys) => new Set(keys).add(task.taskKey));
    try {
      const reviewed = await codeReviewApi.runAi({ taskKey: task.taskKey });
      setActiveTask(reviewed);
      await loadTasks();
      message.success("AI 审查完成");
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setReviewingTaskKeys((keys) => {
        const next = new Set(keys);
        next.delete(task.taskKey);
        return next;
      });
      reviewingTaskKeysRef.current.delete(task.taskKey);
    }
  }

  async function runAiForSelected() {
    const selected = tasks.filter((task) => selectedTaskKeys.includes(task.taskKey));
    const candidates = selected.filter(
      (task) => ["diff_ready", "review_ready"].includes(task.status) && !isTaskSuperseded(task),
    );
    if (candidates.length === 0) {
      message.warning("请选择可执行 AI 审查的任务");
      return;
    }
    setReviewingTaskKeys((keys) => {
      const next = new Set(keys);
      candidates.forEach((task) => {
        reviewingTaskKeysRef.current.add(task.taskKey);
        next.add(task.taskKey);
      });
      return next;
    });
    setLoading(true);
    let successCount = 0;
    try {
      for (const task of candidates) {
        try {
          await codeReviewApi.runAi({ taskKey: task.taskKey });
          successCount += 1;
        } catch (error) {
          message.warning(`${task.workspaceName} AI 审查失败：${getErrorMessage(error)}`);
        }
      }
      await loadTasks();
      message.success(`已完成 ${successCount}/${candidates.length} 个 AI 审查`);
    } finally {
      setReviewingTaskKeys((keys) => {
        const next = new Set(keys);
        candidates.forEach((task) => {
          reviewingTaskKeysRef.current.delete(task.taskKey);
          next.delete(task.taskKey);
        });
        return next;
      });
      setLoading(false);
    }
  }

  async function mergeTask(task: CodeReviewTask) {
    const superseded = getTaskSupersededInfo(task);
    if (superseded) {
      message.warning(`${superseded.reason}，旧任务不能再次合并`);
      return;
    }
    const highRisk = isHighRiskTask(task);
    Modal.confirm({
      title: "确认本地合并",
      content: `${highRisk ? "该任务命中高风险规则，请确认已人工复核。 " : ""}确认将 ${task.sourceBranch} 合并到 ${task.targetBranch}？此操作只执行本地 merge，不会自动推送远程。`,
      okText: "确认合并",
      cancelText: "取消",
      onOk: async () => {
        setLoading(true);
        try {
          const merged = await codeReviewApi.merge(task.taskKey);
          setActiveTask(merged);
          await loadTasks();
          message.success("本地合并完成");
        } catch (error) {
          message.error(getErrorMessage(error));
        } finally {
          setLoading(false);
        }
      },
    });
  }

  async function abortMerge(task: CodeReviewTask) {
    Modal.confirm({
      title: "中止本次合并",
      content: `确认对 ${task.workspaceName} 执行 git merge --abort？`,
      okText: "中止合并",
      cancelText: "取消",
      onOk: async () => {
        setLoading(true);
        try {
          const aborted = await codeReviewApi.abortMerge(task.taskKey);
          setActiveTask(aborted);
          await loadTasks();
          message.success("已中止本次合并");
        } catch (error) {
          message.error(getErrorMessage(error));
        } finally {
          setLoading(false);
        }
      },
    });
  }

  async function cancelTask(task: CodeReviewTask) {
    Modal.confirm({
      title: "放弃审查任务",
      content: `确认放弃 ${task.workspaceName} 的 ${task.sourceBranch} -> ${task.targetBranch} 审查任务？`,
      okText: "放弃",
      okButtonProps: { danger: true },
      cancelText: "取消",
      onOk: async () => {
        setLoading(true);
        try {
          const cancelled = await codeReviewApi.cancel(task.taskKey);
          setActiveTask(cancelled);
          await loadTasks();
          message.success("已放弃任务");
        } catch (error) {
          message.error(getErrorMessage(error));
        } finally {
          setLoading(false);
        }
      },
    });
  }

  async function mergeSelectedTasks() {
    const selected = tasks.filter((task) => selectedTaskKeys.includes(task.taskKey));
    const supersededCount = selected.filter(isTaskSuperseded).length;
    const mergeable = selected.filter(
      (task) => ["review_ready", "merge_failed"].includes(task.status) && !isTaskSuperseded(task),
    );
    const highRiskItems = mergeable.filter(isHighRiskTask);
    const candidates = mergeable.filter((task) => !isHighRiskTask(task));
    if (candidates.length === 0) {
      if (highRiskItems.length > 0) {
        message.warning("选中任务均为高风险任务，请进入单项详情手动确认合并");
      } else {
        message.warning("请选择可确认合并的任务");
      }
      return;
    }
    Modal.confirm({
      title: "确认逐项本地合并",
      content: `将逐项本地合并 ${candidates.length} 个任务。已跳过 ${highRiskItems.length} 个高风险任务、${supersededCount} 个已有最新任务完成的旧任务。该操作不会自动推送远程，失败项不会回滚已成功项。`,
      okText: "逐项合并",
      cancelText: "取消",
      onOk: async () => {
        setLoading(true);
        const summary: BatchRunSummary = {
          success: 0,
          failed: 0,
          conflict: 0,
          stale: 0,
          skipped: selected.length - candidates.length,
          pending: selectedTaskKeys.length - selected.length,
          errors: [],
        };
        try {
          for (const task of candidates) {
            try {
              await codeReviewApi.merge(task.taskKey);
              summary.success += 1;
            } catch (error) {
              const errorMessage = getErrorMessage(error);
              if (errorMessage.includes("变化") || errorMessage.toLowerCase().includes("stale")) {
                summary.stale += 1;
              } else if (errorMessage.includes("冲突") || errorMessage.toLowerCase().includes("conflict")) {
                summary.conflict += 1;
              } else {
                summary.failed += 1;
              }
              const detail = `${task.workspaceName} 合并失败：${errorMessage}`;
              summary.errors.push(detail);
              message.warning(detail);
            }
          }
          setBatchSummary(summary);
          await loadTasks();
          message.success(`已完成 ${summary.success}/${candidates.length} 个本地合并`);
        } finally {
          setLoading(false);
        }
      },
    });
  }

  async function pushTask(task: CodeReviewTask) {
    const superseded = getTaskSupersededInfo(task);
    if (superseded) {
      message.warning(`${superseded.reason}，旧任务不能再次推送`);
      return;
    }
    Modal.confirm({
      title: "推送远程",
      content: `确认推送 ${task.targetBranch} 到远程仓库？`,
      okText: "推送",
      cancelText: "取消",
      onOk: async () => {
        setLoading(true);
        try {
          const pushed = await codeReviewApi.push(task.taskKey);
          setActiveTask(pushed);
          await loadTasks();
          message.success("已推送远程");
        } catch (error) {
          message.error(getErrorMessage(error));
        } finally {
          setLoading(false);
        }
      },
    });
  }

  async function parseBatch() {
    if (!batchText.trim()) {
      message.warning("请输入需要解析的合并说明");
      return;
    }
    setLoading(true);
    try {
      const result = await codeReviewApi.parseBatch({ rawText: batchText });
      const parsedItems = result.items.map((item) => ({
        ...item,
        selectedWorkspaceKey: item.matchedWorkspaceKey ?? undefined,
      }));
      setBatchKey(result.batchKey);
      setBatchItems(parsedItems);
      setBatchSummary(null);
      await Promise.all(
        Array.from(new Set(parsedItems.map((item) => item.selectedWorkspaceKey).filter(Boolean))).map((workspaceKey) =>
          loadWorkspaceBranchesCached(workspaceKey!),
        ),
      );
      if (result.items.length === 0) {
        message.warning("未解析到可用任务");
      } else {
        message.success(`已解析 ${result.items.length} 个任务`);
      }
      if (result.warnings.length) {
        message.warning(result.warnings.join("；"));
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }

  function updateBatchItem(index: number, patch: Partial<BatchDraftItem>) {
    setBatchItems((items) =>
      items.map((item, itemIndex) => (itemIndex === index ? { ...item, ...patch } : item)),
    );
  }

  async function createBatchTasksAndPrepare() {
    const validItems = batchItems.filter((item) => item.selectedWorkspaceKey);
    if (!batchKey || validItems.length === 0) {
      message.warning("请先解析并为任务选择 Git 工作区");
      return;
    }
    setLoading(true);
    try {
      const created = await codeReviewApi.createBatchTasks({
        batchKey,
        items: validItems.map((item) => ({
          workspaceKey: item.selectedWorkspaceKey!,
          projectName: item.projectName,
          sourceBranch: item.sourceBranch,
          targetBranch: item.targetBranch,
        })),
      });
      let successCount = 0;
      const errors: string[] = [];
      for (const task of created) {
        try {
          await codeReviewApi.prepareDiff(task.taskKey);
          successCount += 1;
        } catch (error) {
          const errorMessage = `${task.workspaceName} 生成 Diff 失败：${getErrorMessage(error)}`;
          errors.push(errorMessage);
          message.warning(errorMessage);
        }
      }
      setBatchSummary({
        success: successCount,
        failed: created.length - successCount,
        conflict: 0,
        stale: 0,
        skipped: batchItems.length - validItems.length,
        pending: validItems.length - created.length,
        errors,
      });
      await loadTasks();
      message.success(`已创建 ${created.length} 个任务，成功生成 ${successCount} 个 Diff`);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }

  function openTask(task: CodeReviewTask) {
    setActiveTask(task);
    setDetailOpen(true);
  }

  function copyReport(task: CodeReviewTask) {
    const text = buildReportMarkdown(task);
    void navigator.clipboard.writeText(text);
    message.success("已复制 Markdown 审查报告");
  }

  const taskColumns: ColumnsType<CodeReviewTask> = [
    { title: "工作区", dataIndex: "workspaceName", width: 180 },
    {
      title: "分支",
      width: 240,
      render: (_, row) => (
        <Text>
          {row.sourceBranch} {"->"} {row.targetBranch}
        </Text>
      ),
    },
    {
      title: "状态",
      width: 150,
      render: (_, row) => {
        const superseded = getTaskSupersededInfo(row);
        return (
          <Space size={4} wrap>
            <Tag color={superseded ? "default" : undefined}>{superseded ? "已有最新任务完成" : statusText[row.status] ?? row.status}</Tag>
            <Tag>{pushText[row.pushStatus] ?? row.pushStatus}</Tag>
          </Space>
        );
      },
    },
    {
      title: "风险",
      dataIndex: "riskLevel",
      width: 90,
      render: (value) => <RiskTag value={value} />,
    },
    { title: "更新时间", dataIndex: "updatedAt", width: 170 },
    {
      title: "操作",
      width: 330,
      fixed: "right",
      render: (_, row) => {
        const superseded = getTaskSupersededInfo(row);
        const supersededTitle = superseded ? `${superseded.reason}，最新任务：${superseded.task.taskKey}` : "";
        return (
          <Space size={6} wrap>
            <Button icon={<Eye size={14} />} size="small" onClick={() => openTask(row)}>
              查看
            </Button>
            <Tooltip title={supersededTitle}>
              <Button
                icon={<Bot size={14} />}
                size="small"
                loading={reviewingTaskKeys.has(row.taskKey)}
                disabled={
                  Boolean(superseded) ||
                  !["diff_ready", "review_ready"].includes(row.status) ||
                  reviewingTaskKeys.has(row.taskKey)
                }
                onClick={() => void runAi(row)}
              >
                AI 审查
              </Button>
            </Tooltip>
            <Tooltip title={supersededTitle || (isHighRiskTask(row) ? "高风险任务请进入详情页手动确认" : "")}>
              <Button
                icon={<GitMerge size={14} />}
                size="small"
                disabled={
                  Boolean(superseded) ||
                  !["review_ready", "merge_failed"].includes(row.status) ||
                  isHighRiskTask(row)
                }
                onClick={() => void mergeTask(row)}
              >
                合并
              </Button>
            </Tooltip>
            <Tooltip title={supersededTitle}>
              <Button
                icon={<Upload size={14} />}
                size="small"
                disabled={Boolean(superseded) || row.status !== "merged" || row.pushStatus === "pushed"}
                onClick={() => void pushTask(row)}
              >
                推送
              </Button>
            </Tooltip>
          </Space>
        );
      },
    },
  ];

  const fileColumns: ColumnsType<CodeReviewChangedFile> = [
    { title: "文件", dataIndex: "path", ellipsis: true },
    { title: "状态", dataIndex: "status", width: 90 },
    {
      title: "+",
      dataIndex: "additions",
      width: 80,
      render: (value) => <ChangeNumber value={value} type="add" />,
    },
    {
      title: "-",
      dataIndex: "deletions",
      width: 80,
      render: (value) => <ChangeNumber value={value} type="delete" />,
    },
  ];

  const batchColumns: ColumnsType<BatchDraftItem> = [
    { title: "项目", dataIndex: "projectName" },
    {
      title: "源分支",
      dataIndex: "sourceBranch",
      width: 220,
      render: (value, row, index) => (
        <Select
          showSearch
          optionFilterProp="label"
          value={value || undefined}
          placeholder={row.selectedWorkspaceKey ? "选择源分支" : "先选择 Git 工作区"}
          disabled={!row.selectedWorkspaceKey}
          options={batchBranchOptions(row.selectedWorkspaceKey, value)}
          style={{ width: 200 }}
          onChange={(sourceBranch) => updateBatchItem(index, { sourceBranch })}
        />
      ),
    },
    {
      title: "目标分支",
      dataIndex: "targetBranch",
      width: 220,
      render: (value, row, index) => (
        <Select
          showSearch
          optionFilterProp="label"
          value={value || undefined}
          placeholder={row.selectedWorkspaceKey ? "选择目标分支" : "先选择 Git 工作区"}
          disabled={!row.selectedWorkspaceKey}
          options={batchBranchOptions(row.selectedWorkspaceKey, value)}
          style={{ width: 200 }}
          onChange={(targetBranch) => updateBatchItem(index, { targetBranch })}
        />
      ),
    },
    {
      title: "Git 工作区",
      dataIndex: "selectedWorkspaceKey",
      width: 280,
      render: (value, _row, index) => (
        <Select
          showSearch
          optionFilterProp="label"
          value={value || undefined}
          placeholder="选择 Git 工作区"
          options={workspaceOptions}
          style={{ width: 260 }}
          onChange={(selectedWorkspaceKeyValue) => {
            void loadWorkspaceBranchesCached(selectedWorkspaceKeyValue);
            updateBatchItem(index, {
              selectedWorkspaceKey: selectedWorkspaceKeyValue,
              status: "matched",
            });
          }}
        />
      ),
    },
    {
      title: "最后提交",
      width: 320,
      render: (_, row) => (
        <Space direction="vertical" size={2}>
          <Text type="secondary">源：{branchLastCommitText(row.selectedWorkspaceKey, row.sourceBranch)}</Text>
          <Text type="secondary">目标：{branchLastCommitText(row.selectedWorkspaceKey, row.targetBranch)}</Text>
        </Space>
      ),
    },
    { title: "置信度", dataIndex: "confidence", render: (value) => `${Math.round(value * 100)}%` },
    {
      title: "状态",
      dataIndex: "status",
      render: (value, row) => (
        <Space size={4} wrap>
          <Tag color={row.selectedWorkspaceKey ? "green" : "orange"}>
            {batchStatusText[value] ?? `未知状态：${value}`}
          </Tag>
          {row.warnings.map((warning) => (
            <Tag key={warning} color="gold">
              {warning}
            </Tag>
          ))}
        </Space>
      ),
    },
  ];

  return (
    <div className="prototype-page">
      <PageHeader
        title="代码审核"
        description="基于本地 Git 工作区生成分支差异、调用 AI 审查，并在用户确认后执行受控合并。"
      />

      <Tabs
        items={[
          {
            key: "single",
            label: "单项目审核",
            children: (
              <Space direction="vertical" size={16} className="w-full">
                <Card title="创建审查任务">
                  <Form form={form} layout="vertical" requiredMark={false}>
                    <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_190px]">
                      <div className="grid gap-3 md:grid-cols-3">
                        <Form.Item
                          className="mb-0"
                          label="Git 工作区"
                          name="workspaceKey"
                          rules={[{ required: true, message: "请选择 Git 工作区" }]}
                        >
                          <Select
                            showSearch
                            optionFilterProp="label"
                            options={workspaceOptions}
                            placeholder="选择 Git 工作区"
                          />
                        </Form.Item>
                        <Form.Item
                          className="mb-0"
                          label="源分支"
                          name="sourceBranch"
                          rules={[{ required: true, message: "请选择源分支" }]}
                        >
                          <Select
                            showSearch
                            optionFilterProp="label"
                            options={branchOptions}
                            placeholder="选择源分支"
                          />
                        </Form.Item>
                        <Form.Item
                          className="mb-0"
                          label="目标分支"
                          name="targetBranch"
                          rules={[{ required: true, message: "请选择目标分支" }]}
                        >
                          <Select
                            showSearch
                            optionFilterProp="label"
                            options={branchOptions}
                            placeholder="选择目标分支"
                          />
                        </Form.Item>
                      </div>
                      <div className="flex items-end justify-start border-t border-slate-100 pt-3 lg:justify-end lg:border-l lg:border-t-0 lg:pl-4 lg:pt-0">
                        <div className="flex w-full gap-2 lg:flex-col">
                          <Button
                            block
                            icon={<RefreshCw size={14} />}
                            onClick={() => void loadBranches(selectedWorkspaceKey)}
                          >
                            刷新分支
                          </Button>
                          <Button
                            block
                            type="primary"
                            icon={<Search size={14} />}
                            loading={loading}
                            onClick={() => void createAndPrepare()}
                          >
                            生成审查
                          </Button>
                        </div>
                      </div>
                    </div>
                  </Form>
                </Card>
                <Card title="当前选择状态">
                  {selectedWorkspace ? (
                    <Descriptions bordered size="small" column={3}>
                      <Descriptions.Item label="工作区">{selectedWorkspace.name}</Descriptions.Item>
                      <Descriptions.Item label="当前分支">
                        {selectedWorkspace.branch || "-"}
                      </Descriptions.Item>
                      <Descriptions.Item label="状态">{selectedWorkspace.status || "-"}</Descriptions.Item>
                      <Descriptions.Item label="未提交文件">{selectedWorkspace.changedFiles}</Descriptions.Item>
                      <Descriptions.Item label="Ahead / Behind">
                        {selectedWorkspace.ahead} / {selectedWorkspace.behind}
                      </Descriptions.Item>
                      <Descriptions.Item label="远程">
                        <Text ellipsis>{selectedWorkspace.remoteUrl || "-"}</Text>
                      </Descriptions.Item>
                      <Descriptions.Item label="源分支最后提交">
                        {selectedSourceBranchInfo ? (
                          <Space direction="vertical" size={2}>
                            <Text>
                              {selectedSourceBranchInfo.lastCommitHash} {selectedSourceBranchInfo.lastCommitMessage}
                            </Text>
                            <Text type="secondary">
                              更新时间：{formatCommitUpdatedAt(selectedSourceBranchInfo.lastCommitAt)}
                            </Text>
                          </Space>
                        ) : (
                          "-"
                        )}
                      </Descriptions.Item>
                      <Descriptions.Item label="目标分支最后提交" span={2}>
                        {selectedTargetBranchInfo ? (
                          <Space direction="vertical" size={2}>
                            <Text>
                              {selectedTargetBranchInfo.lastCommitHash} {selectedTargetBranchInfo.lastCommitMessage}
                            </Text>
                            <Text type="secondary">
                              更新时间：{formatCommitUpdatedAt(selectedTargetBranchInfo.lastCommitAt)}
                            </Text>
                          </Space>
                        ) : (
                          "-"
                        )}
                      </Descriptions.Item>
                    </Descriptions>
                  ) : (
                    <Empty description="请选择 Git 工作区" />
                  )}
                </Card>
                <Card
                  title="审查任务"
                  extra={
                    <Space>
                      <Button
                        icon={<Bot size={14} />}
                        disabled={selectedTaskKeys.length === 0}
                        loading={loading}
                        onClick={() => void runAiForSelected()}
                      >
                        AI 审查选中
                      </Button>
                      <Button
                        icon={<GitMerge size={14} />}
                        disabled={selectedTaskKeys.length === 0}
                        loading={loading}
                        onClick={() => void mergeSelectedTasks()}
                      >
                        合并选中
                      </Button>
                      <Button icon={<RefreshCw size={14} />} onClick={() => void loadTasks()}>
                        刷新
                      </Button>
                    </Space>
                  }
                >
                  <Table
                    rowKey="taskKey"
                    columns={taskColumns}
                    dataSource={tasks}
                    rowSelection={{
                      selectedRowKeys: selectedTaskKeys,
                      onChange: setSelectedTaskKeys,
                    }}
                    scroll={{ x: 1120 }}
                    pagination={{ pageSize: 8 }}
                  />
                </Card>
              </Space>
            ),
          },
          {
            key: "batch",
            label: "批量解析",
            children: (
              <Space direction="vertical" size={16} className="w-full">
                <Alert
                  type="info"
                  showIcon
                  message="批量解析先用规则解析，规则无法识别时会调用 AI 兜底；解析结果只生成可编辑计划，不会自动执行合并。"
                />
                <Card title="粘贴合并说明">
                  <Space direction="vertical" size={12} className="w-full">
                    <Input.TextArea
                      rows={6}
                      value={batchText}
                      onChange={(event) => setBatchText(event.target.value)}
                      placeholder="例如：前端项目：fj-evaluate-app 分支：dev-v3.7，后端项目：fj-evaluate、fj-feb 分支：dev-过错认定v3.7，都合并 dev 分支"
                    />
                    <Button icon={<GitPullRequestArrow size={14} />} loading={loading} onClick={() => void parseBatch()}>
                      解析合并任务
                    </Button>
                  </Space>
                </Card>
                <Card title="解析结果">
                  <div className="mb-3 flex items-center justify-between gap-3">
                    <Text type="secondary">
                      {batchKey ? `批次：${batchKey}` : "解析后可在表格中修正分支并选择 Git 工作区。"}
                    </Text>
                    <Button
                      type="primary"
                      icon={<Search size={14} />}
                      loading={loading}
                      disabled={!batchItems.some((item) => item.selectedWorkspaceKey)}
                      onClick={() => void createBatchTasksAndPrepare()}
                    >
                      创建任务并生成 Diff
                    </Button>
                  </div>
                  <Table
                    rowKey={(row) => `${row.projectName}-${row.sourceBranch}-${row.targetBranch}`}
                    columns={batchColumns}
                    dataSource={batchItems}
                    scroll={{ x: 1100 }}
                    locale={{ emptyText: <Empty description="暂无解析结果" /> }}
                  />
                  {batchSummary ? (
                    <Alert
                      className="mt-3"
                      type={batchSummary.failed || batchSummary.conflict || batchSummary.stale ? "warning" : "success"}
                      showIcon
                      message={`批量结果：成功 ${batchSummary.success}，失败 ${batchSummary.failed}，冲突 ${batchSummary.conflict}，stale ${batchSummary.stale}，跳过 ${batchSummary.skipped}，待确认 ${batchSummary.pending}`}
                      description={
                        batchSummary.errors.length ? (
                          <Space direction="vertical" size={2}>
                            {batchSummary.errors.map((error) => (
                              <Text key={error} type="secondary">
                                {error}
                              </Text>
                            ))}
                          </Space>
                        ) : null
                      }
                    />
                  ) : null}
                </Card>
              </Space>
            ),
          },
          {
            key: "records",
            label: "审查记录",
            children: (
              <Card title="审查记录">
                <Table
                  rowKey="taskKey"
                  columns={taskColumns}
                  dataSource={tasks}
                  rowSelection={{
                    selectedRowKeys: selectedTaskKeys,
                    onChange: setSelectedTaskKeys,
                  }}
                  scroll={{ x: 1120 }}
                  pagination={{ pageSize: 10 }}
                />
              </Card>
            ),
          },
        ]}
      />

      <Drawer
        title={activeTask ? `审查详情：${activeTask.workspaceName}` : "审查详情"}
        width={920}
        open={detailOpen}
        onClose={() => setDetailOpen(false)}
        extra={
          activeTask ? (
            (() => {
              const superseded = getTaskSupersededInfo(activeTask);
              const supersededTitle = superseded ? `${superseded.reason}，最新任务：${superseded.task.taskKey}` : "";
              const hasAiReviewReport = Boolean(activeTask.aiReviewMarkdown.trim());
              return (
                <Space>
                  <Tooltip title={hasAiReviewReport ? "" : "请先完成 AI 审查后再复制报告"}>
                    <Button
                      icon={<ClipboardCopy size={14} />}
                      disabled={!hasAiReviewReport}
                      onClick={() => copyReport(activeTask)}
                    >
                      复制报告
                    </Button>
                  </Tooltip>
                  <Tooltip title={supersededTitle}>
                    <Button
                      icon={<Bot size={14} />}
                      loading={reviewingTaskKeys.has(activeTask.taskKey)}
                      disabled={
                        Boolean(superseded) ||
                        !["diff_ready", "review_ready"].includes(activeTask.status) ||
                        reviewingTaskKeys.has(activeTask.taskKey)
                      }
                      onClick={() => void runAi(activeTask)}
                    >
                      AI 审查
                    </Button>
                  </Tooltip>
                  <Tooltip title={supersededTitle}>
                    <Button
                      icon={<ShieldCheck size={14} />}
                      disabled={Boolean(superseded) || !["review_ready", "merge_failed"].includes(activeTask.status)}
                      onClick={() => void mergeTask(activeTask)}
                    >
                      确认合并
                    </Button>
                  </Tooltip>
                  <Tooltip title={supersededTitle}>
                    <Button
                      icon={<Upload size={14} />}
                      loading={loading && activeTask.status === "merged" && activeTask.pushStatus !== "pushed"}
                      disabled={
                        Boolean(superseded) ||
                        activeTask.status !== "merged" ||
                        activeTask.pushStatus === "pushed" ||
                        loading
                      }
                      onClick={() => void pushTask(activeTask)}
                    >
                      Push 推送
                    </Button>
                  </Tooltip>
                  {["conflict", "merge_failed"].includes(activeTask.status) ? (
                    <Button icon={<Undo2 size={14} />} onClick={() => void abortMerge(activeTask)}>
                      中止合并
                    </Button>
                  ) : null}
                  {activeTask.status === "cancelled" ? (
                    <Button onClick={() => setDetailOpen(false)}>
                      关闭
                    </Button>
                  ) : activeTask.status !== "merged" && activeTask.pushStatus !== "pushed" ? (
                    <Button danger icon={<Ban size={14} />} onClick={() => void cancelTask(activeTask)}>
                      放弃
                    </Button>
                  ) : null}
                </Space>
              );
            })()
          ) : null
        }
      >
        {activeTask ? (
          <Space direction="vertical" size={16} className="w-full">
            <Descriptions bordered size="small" column={2}>
              <Descriptions.Item label="工作区">{activeTask.workspaceName}</Descriptions.Item>
              <Descriptions.Item label="状态">{statusText[activeTask.status] ?? activeTask.status}</Descriptions.Item>
              <Descriptions.Item label="源分支">{activeTask.sourceBranch}</Descriptions.Item>
              <Descriptions.Item label="目标分支">{activeTask.targetBranch}</Descriptions.Item>
              <Descriptions.Item label="源 HEAD">{activeTask.sourceHead || "-"}</Descriptions.Item>
              <Descriptions.Item label="目标 HEAD">{activeTask.targetHead || "-"}</Descriptions.Item>
              <Descriptions.Item label="风险">
                <RiskTag value={activeTask.riskLevel} />
              </Descriptions.Item>
              <Descriptions.Item label="推送状态">
                {pushText[activeTask.pushStatus] ?? activeTask.pushStatus}
              </Descriptions.Item>
            </Descriptions>
            {isHighRiskTask(activeTask) ? (
              <Alert
                type="warning"
                showIcon
                message="该任务命中高风险规则，只允许在详情页人工复核后单项确认合并。"
              />
            ) : null}
            {getTaskSupersededInfo(activeTask) ? (
              <Alert
                type="info"
                showIcon
                message={getTaskSupersededInfo(activeTask)!.reason}
                description={`最新任务：${getTaskSupersededInfo(activeTask)!.task.taskKey}。当前旧任务不会再允许 AI 审查、合并或推送。`}
              />
            ) : null}
            {activeTask.errorMessage ? <Alert type="error" showIcon message={activeTask.errorMessage} /> : null}
            <Card title="文件变更">
              <Table
                rowKey="path"
                columns={fileColumns}
                dataSource={activeTask.changedFiles}
                pagination={{ pageSize: 8 }}
              />
            </Card>
            <Card title="Diff 片段">
              {getDiffExcerpts(activeTask).length ? (
                <Space direction="vertical" size={12} className="w-full">
                  {getDiffExcerpts(activeTask).map((excerpt) => (
                    <Card key={excerpt.path} size="small" title={excerpt.path}>
                      {excerpt.truncated ? <Tag color="gold">已截断</Tag> : null}
                      <DiffCodeBlock path={excerpt.path} content={excerpt.content} />
                    </Card>
                  ))}
                </Space>
              ) : (
                <Empty description="暂无 Diff 片段" />
              )}
            </Card>
            <Card title="提交列表">
              {activeTask.commits.length ? (
                <Space direction="vertical" size={8} className="w-full">
                  {activeTask.commits.map((commit) => (
                    <div key={commit.hash} className="flex items-start gap-2">
                      <GitBranch size={14} className="mt-1" />
                      <div>
                        <Text code>{commit.hash}</Text> <Text>{commit.message}</Text>
                        <br />
                        <Text type="secondary">
                          {commit.author} / {commit.date}
                        </Text>
                      </div>
                    </div>
                  ))}
                </Space>
              ) : (
                <Empty description="暂无提交" />
              )}
            </Card>
            <Card title="AI 审查报告">
              {activeTask.aiReviewMarkdown ? (
                <MarkdownReport content={activeTask.aiReviewMarkdown} />
              ) : (
                <Empty description="尚未运行 AI 审查" />
              )}
            </Card>
          </Space>
        ) : null}
      </Drawer>
    </div>
  );
}

function getDiffExcerpts(task: CodeReviewTask): DiffExcerpt[] {
  if (!Array.isArray(task.diffExcerpt)) {
    return [];
  }
  return task.diffExcerpt
    .filter((item): item is DiffExcerpt => {
      return (
        typeof item === "object" &&
        item !== null &&
        "path" in item &&
        "content" in item &&
        typeof (item as DiffExcerpt).path === "string" &&
        typeof (item as DiffExcerpt).content === "string"
      );
    })
    .slice(0, 20);
}

function reviewGroupKey(task: CodeReviewTask) {
  return [task.workspaceKey, task.sourceBranch, task.targetBranch].join("\u0001");
}

function ChangeNumber({ value, type }: { value?: number; type: "add" | "delete" }) {
  const count = Number(value ?? 0);
  const color = type === "add" ? "#16a34a" : "#dc2626";
  return (
    <span style={{ color, fontWeight: 700 }}>
      {count}
    </span>
  );
}

interface DiffRenderLine {
  key: string;
  oldLine?: number;
  newLine?: number;
  marker: string;
  content: string;
  tone: "add" | "delete" | "hunk" | "meta" | "context";
}

function DiffCodeBlock({ path, content }: { path: string; content?: string }) {
  const lines = parseDiffLines(content || "无文本 diff");
  const language = languageFromPath(path);
  return (
    <div className="mt-2 max-h-96 overflow-auto rounded border border-slate-200 bg-slate-50 font-mono text-xs leading-5">
      {lines.map((line) => (
        <div
          key={line.key}
          className={[
            "grid min-w-max grid-cols-[56px_56px_28px_minmax(760px,1fr)] whitespace-pre",
            line.tone === "add" ? "bg-green-50" : "",
            line.tone === "delete" ? "bg-red-50" : "",
            line.tone === "hunk" ? "bg-blue-50" : "",
            line.tone === "meta" ? "bg-slate-100" : "",
            line.tone === "context" ? "bg-slate-50" : "",
          ]
            .filter(Boolean)
            .join(" ")}
        >
          <span className="select-none border-r border-slate-200 px-2 text-right text-slate-400">
            {line.oldLine ?? ""}
          </span>
          <span className="select-none border-r border-slate-200 px-2 text-right text-slate-400">
            {line.newLine ?? ""}
          </span>
          <span
            className={[
              "select-none border-r border-slate-200 px-2 text-center",
              line.tone === "add" ? "text-green-600" : "",
              line.tone === "delete" ? "text-red-600" : "",
              line.tone === "hunk" ? "text-blue-600" : "",
              line.tone === "meta" ? "text-slate-400" : "",
            ]
              .filter(Boolean)
              .join(" ")}
          >
            {line.marker}
          </span>
          <code
            className={[
              "px-3 py-0.5",
              line.tone === "add" ? "text-green-700" : "",
              line.tone === "delete" ? "text-red-700" : "",
              line.tone === "hunk" ? "font-semibold text-blue-700" : "",
              line.tone === "meta" ? "text-slate-500" : "",
              line.tone === "context" ? "text-slate-800" : "",
            ]
              .filter(Boolean)
              .join(" ")}
          >
            {line.tone === "hunk" || line.tone === "meta"
              ? line.content || " "
              : highlightCode(line.content || " ", language)}
          </code>
        </div>
      ))}
    </div>
  );
}

function parseDiffLines(content: string): DiffRenderLine[] {
  let oldLine = 0;
  let newLine = 0;
  return content.split("\n").map((raw, index) => {
    const tone = diffLineTone(raw);
    if (tone === "hunk") {
      const match = raw.match(/^@@\s+-(\d+)(?:,\d+)?\s+\+(\d+)(?:,\d+)?\s+@@/);
      if (match) {
        oldLine = Number(match[1]);
        newLine = Number(match[2]);
      }
      return {
        key: `${index}-hunk`,
        marker: "@",
        content: raw,
        tone,
      };
    }

    if (tone === "meta") {
      return {
        key: `${index}-meta`,
        marker: "",
        content: raw,
        tone,
      };
    }

    const contentText = raw.startsWith("+") || raw.startsWith("-") || raw.startsWith(" ")
      ? raw.slice(1)
      : raw;
    if (tone === "add") {
      return {
        key: `${index}-add-${newLine}`,
        newLine: newLine++,
        marker: "+",
        content: contentText,
        tone,
      };
    }
    if (tone === "delete") {
      return {
        key: `${index}-delete-${oldLine}`,
        oldLine: oldLine++,
        marker: "-",
        content: contentText,
        tone,
      };
    }
    const line = {
      key: `${index}-context-${oldLine}-${newLine}`,
      oldLine: oldLine || undefined,
      newLine: newLine || undefined,
      marker: "",
      content: contentText,
      tone,
    };
    if (oldLine > 0) {
      oldLine += 1;
    }
    if (newLine > 0) {
      newLine += 1;
    }
    return line;
  });
}

function diffLineTone(line: string): DiffRenderLine["tone"] {
  if (line.startsWith("@@")) {
    return "hunk";
  }
  if (line.startsWith("diff --git") || line.startsWith("index ") || line.startsWith("--- ") || line.startsWith("+++ ")) {
    return "meta";
  }
  if (line.startsWith("+")) {
    return "add";
  }
  if (line.startsWith("-")) {
    return "delete";
  }
  return "context";
}

function languageFromPath(path: string) {
  const lower = path.toLowerCase();
  if (lower.endsWith(".java")) return "java";
  if (lower.endsWith(".xml")) return "xml";
  if (lower.endsWith(".sql")) return "sql";
  if (lower.endsWith(".ts") || lower.endsWith(".tsx") || lower.endsWith(".js") || lower.endsWith(".jsx")) return "ts";
  if (lower.endsWith(".json")) return "json";
  if (lower.endsWith(".yml") || lower.endsWith(".yaml")) return "yaml";
  return "plain";
}

function highlightCode(code: string, language: string) {
  const tokenPattern =
    language === "xml"
      ? /(<!--[\s\S]*?-->|<\/?[\w:.-]+|\/?>|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|\b(?:select|from|where|order|by|and|or|insert|update|delete|mapper|resultType|namespace|id)\b|`[^`]+`)/gi
      : /("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|\/\/.*$|\/\*[\s\S]*?\*\/|\b(?:class|interface|public|private|protected|static|final|void|return|if|else|for|while|try|catch|new|import|package|throws|throw|extends|implements|const|let|var|function|async|await|type|interface|select|from|where|order|by|and|or)\b|\b\d+(?:\.\d+)?\b|@[A-Za-z_]\w*)/gm;
  const nodes = [];
  let cursor = 0;
  for (const match of code.matchAll(tokenPattern)) {
    const value = match[0];
    const start = match.index ?? 0;
    if (start > cursor) {
      nodes.push(code.slice(cursor, start));
    }
    nodes.push(
      <span key={`${start}-${value}`} className={syntaxTokenClass(value, language)}>
        {value}
      </span>,
    );
    cursor = start + value.length;
  }
  if (cursor < code.length) {
    nodes.push(code.slice(cursor));
  }
  return nodes.length ? nodes : code;
}

function syntaxTokenClass(token: string, language: string) {
  if (token.startsWith("\"") || token.startsWith("'") || token.startsWith("`")) {
    return "text-amber-700";
  }
  if (token.startsWith("//") || token.startsWith("/*") || token.startsWith("<!--")) {
    return "text-slate-400";
  }
  if (token.startsWith("@")) {
    return "text-purple-600";
  }
  if (/^\d/.test(token)) {
    return "text-cyan-700";
  }
  if (language === "xml" && (token.startsWith("<") || token === ">" || token === "/>")) {
    return "text-blue-700";
  }
  return "text-violet-700";
}

function MarkdownReport({ content }: { content: string }) {
  return <div className="space-y-3 text-sm leading-7 text-slate-800">{renderMarkdownBlocks(content)}</div>;
}

function renderMarkdownBlocks(markdown: string) {
  const lines = markdown.replace(/\r\n/g, "\n").split("\n");
  const blocks = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];
    const trimmed = line.trim();
    if (!trimmed) {
      index += 1;
      continue;
    }

    if (trimmed.startsWith("```")) {
      const language = trimmed.slice(3).trim() || "plain";
      const codeLines = [];
      index += 1;
      while (index < lines.length && !lines[index].trim().startsWith("```")) {
        codeLines.push(lines[index]);
        index += 1;
      }
      index += 1;
      blocks.push(
        <pre
          key={`code-${index}`}
          className="overflow-auto rounded border border-slate-200 bg-slate-50 p-3 font-mono text-xs leading-5 text-slate-800"
        >
          <code>{highlightCode(codeLines.join("\n"), normalizeMarkdownLanguage(language))}</code>
        </pre>,
      );
      continue;
    }

    const heading = trimmed.match(/^(#{1,6})\s+(.+)$/);
    if (heading) {
      const level = heading[1].length;
      const className =
        level <= 2
          ? "mt-1 text-lg font-semibold text-slate-950"
          : "mt-1 text-base font-semibold text-slate-900";
      blocks.push(
        <div key={`heading-${index}`} className={className}>
          {renderInlineMarkdown(heading[2])}
        </div>,
      );
      index += 1;
      continue;
    }

    if (/^[-*_]{3,}$/.test(trimmed)) {
      blocks.push(<div key={`hr-${index}`} className="my-3 border-t border-slate-200" />);
      index += 1;
      continue;
    }

    if (/^[-*]\s+/.test(trimmed)) {
      const items = [];
      while (index < lines.length && /^[-*]\s+/.test(lines[index].trim())) {
        items.push(lines[index].trim().replace(/^[-*]\s+/, ""));
        index += 1;
      }
      blocks.push(
        <ul key={`ul-${index}`} className="list-disc space-y-1 pl-5">
          {items.map((item, itemIndex) => (
            <li key={`${index}-${itemIndex}`}>{renderInlineMarkdown(item)}</li>
          ))}
        </ul>,
      );
      continue;
    }

    if (/^\d+\.\s+/.test(trimmed)) {
      const items = [];
      while (index < lines.length && /^\d+\.\s+/.test(lines[index].trim())) {
        items.push(lines[index].trim().replace(/^\d+\.\s+/, ""));
        index += 1;
      }
      blocks.push(
        <ol key={`ol-${index}`} className="list-decimal space-y-1 pl-5">
          {items.map((item, itemIndex) => (
            <li key={`${index}-${itemIndex}`}>{renderInlineMarkdown(item)}</li>
          ))}
        </ol>,
      );
      continue;
    }

    const paragraph = [trimmed];
    index += 1;
    while (
      index < lines.length &&
      lines[index].trim() &&
      !lines[index].trim().startsWith("```") &&
      !/^(#{1,6})\s+/.test(lines[index].trim()) &&
      !/^[-*]\s+/.test(lines[index].trim()) &&
      !/^\d+\.\s+/.test(lines[index].trim()) &&
      !/^[-*_]{3,}$/.test(lines[index].trim())
    ) {
      paragraph.push(lines[index].trim());
      index += 1;
    }
    blocks.push(
      <p key={`p-${index}`} className="m-0">
        {renderInlineMarkdown(paragraph.join(" "))}
      </p>,
    );
  }

  return blocks;
}

function renderInlineMarkdown(text: string) {
  const parts = [];
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*)/g;
  let cursor = 0;
  for (const match of text.matchAll(pattern)) {
    const value = match[0];
    const start = match.index ?? 0;
    if (start > cursor) {
      parts.push(text.slice(cursor, start));
    }
    if (value.startsWith("`")) {
      parts.push(
        <code key={`${start}-code`} className="rounded bg-slate-100 px-1 py-0.5 font-mono text-[0.92em] text-slate-900">
          {value.slice(1, -1)}
        </code>,
      );
    } else {
      parts.push(
        <strong key={`${start}-strong`} className="font-semibold text-slate-950">
          {value.slice(2, -2)}
        </strong>,
      );
    }
    cursor = start + value.length;
  }
  if (cursor < text.length) {
    parts.push(text.slice(cursor));
  }
  return parts.length ? parts : text;
}

function normalizeMarkdownLanguage(language: string) {
  const normalized = language.toLowerCase();
  if (["java", "xml", "sql", "json", "yaml"].includes(normalized)) {
    return normalized;
  }
  if (["ts", "tsx", "js", "jsx", "typescript", "javascript"].includes(normalized)) {
    return "ts";
  }
  return "plain";
}

function buildReportMarkdown(task: CodeReviewTask) {
  return `# 代码审核报告

- 项目：${task.workspaceName}
- 源分支：${task.sourceBranch}
- 目标分支：${task.targetBranch}
- 风险等级：${formatRiskText(task.riskLevel)}
- 状态：${statusText[task.status] ?? task.status}
- AI 模型：${task.aiProvider || "-"} / ${task.aiModel || "-"}
- 审查时间：${task.updatedAt}

## 审查结果

${task.aiReviewMarkdown || "尚未生成 AI 审查报告。"}
`;
}

function formatCommitUpdatedAt(value: string | null | undefined) {
  if (!value) {
    return "-";
  }
  const date = parseGitCommitTime(value);
  if (!date) {
    return value.replace(/\s+[+-]\d{4}$/, "");
  }
  const absolute = date.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
  return `${absolute}（${formatRelativeTime(date)}）`;
}

function parseGitCommitTime(value: string) {
  const trimmed = value.trim();
  const gitTimeMatch = trimmed.match(
    /^(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})\s+([+-])(\d{2})(\d{2})$/,
  );
  const normalized = gitTimeMatch
    ? `${gitTimeMatch[1]}T${gitTimeMatch[2]}${gitTimeMatch[3]}${gitTimeMatch[4]}:${gitTimeMatch[5]}`
    : trimmed.includes("T")
      ? trimmed
      : trimmed.replace(" ", "T");
  const date = new Date(normalized);
  return Number.isNaN(date.getTime()) ? null : date;
}

function formatRelativeTime(date: Date) {
  const timestamp = date.getTime();
  const diffMs = Math.max(0, Date.now() - timestamp);
  const diffMinutes = Math.floor(diffMs / 60_000);
  if (diffMinutes < 1) {
    return "刚刚";
  }
  if (diffMinutes < 60) {
    return `${diffMinutes} 分钟前`;
  }
  const diffHours = Math.floor(diffMinutes / 60);
  if (diffHours < 24) {
    return `${diffHours} 小时前`;
  }
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 30) {
    return `${diffDays} 天前`;
  }
  const diffMonths = Math.floor(diffDays / 30);
  if (diffMonths < 12) {
    return `${diffMonths} 个月前`;
  }
  return `${Math.floor(diffMonths / 12)} 年前`;
}

function isHighRiskReview(riskLevel: string, targetBranch: string) {
  const normalizedRisk = riskLevel.toLowerCase();
  const normalizedBranch = targetBranch.toLowerCase();
  return (
    normalizedRisk === "critical" ||
    normalizedRisk === "high" ||
    normalizedBranch === "main" ||
    normalizedBranch === "master" ||
    normalizedBranch === "production" ||
    normalizedBranch.startsWith("release/") ||
    normalizedBranch.startsWith("prod/")
  );
}

function formatRiskText(value?: string) {
  if (!value) {
    return "未评估";
  }
  return riskText[value] ?? "未评估";
}

function RiskTag({ value }: { value?: string }) {
  return <Tag color={riskColor[value ?? "unknown"] ?? "default"}>{formatRiskText(value)}</Tag>;
}
