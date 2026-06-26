import { useEffect, useMemo, useState } from "react";
import {
  Badge,
  Button,
  Card,
  Collapse,
  Drawer,
  Empty,
  Form,
  Input,
  InputNumber,
  Modal,
  Select,
  Space,
  Switch,
  Table,
  Tabs,
  Tag,
  Typography,
  message,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import { BookOpen, Copy, FilePlus, RefreshCw, RotateCcw, Search, Sparkles, Trash2 } from "lucide-react";
import { aiSkillApi, getErrorMessage } from "@/lib/api";
import type {
  AiExperience,
  AiRunbook,
  AiRunbookRunResult,
  AiRunbookStep,
  AiSkill,
  AiSkillPromptPreviewResult,
  AiSkillScope,
  AiSkillTriggerResult,
  UpsertAiExperienceInput,
  UpsertAiRunbookInput,
  UpsertAiSkillInput,
} from "@/types";

const scopeOptions: { label: string; value: AiSkillScope }[] = [
  { label: "全局", value: "global" },
  { label: "终端 AI", value: "terminal" },
  { label: "SQL 控制台 AI", value: "sql" },
  { label: "日志解释 AI", value: "logs" },
  { label: "SFTP 文件 AI", value: "sftp" },
  { label: "MCP Agent", value: "mcp" },
  { label: "堡垒机会话", value: "jumpserver" },
];

const stepTypeOptions = [
  { label: "说明", value: "note" },
  { label: "只读命令", value: "readonly_command" },
  { label: "需确认命令", value: "approval_command" },
  { label: "文件操作", value: "file" },
  { label: "SQL", value: "sql" },
  { label: "Redis", value: "redis" },
];

const riskOptions = [
  { label: "低", value: "low" },
  { label: "中", value: "medium" },
  { label: "高", value: "high" },
  { label: "禁止", value: "blocked" },
];

function sourceTag(skill: AiSkill) {
  if (skill.builtin) {
    return <Tag color={skill.userOverridden ? "gold" : "blue"}>{skill.userOverridden ? "内置已覆盖" : "内置"}</Tag>;
  }
  return <Tag color="green">用户</Tag>;
}

function scopeTags(scopes: string[]) {
  return scopes.map((scope) => {
    const label = scopeOptions.find((item) => item.value === scope)?.label ?? scope;
    return <Tag key={scope}>{label}</Tag>;
  });
}

function splitTags(value?: string[] | null) {
  return (value ?? []).map((item) => item.trim()).filter(Boolean);
}

function formatRunbookOutput(output: unknown) {
  if (output === null || output === undefined) {
    return "";
  }
  if (typeof output === "string") {
    return output;
  }
  return JSON.stringify(output, null, 2);
}

function runbookStatusTag(status: string) {
  const colorMap: Record<string, string> = {
    success: "green",
    planned: "blue",
    approval_required: "gold",
    blocked: "red",
    error: "red",
  };
  const labelMap: Record<string, string> = {
    success: "成功",
    planned: "预演",
    approval_required: "需审批",
    blocked: "已禁止",
    error: "失败",
  };
  return <Tag color={colorMap[status] ?? "default"}>{labelMap[status] ?? status}</Tag>;
}

export default function SkillsPage() {
  const [activeTab, setActiveTab] = useState("skills");
  return (
    <div className="prototype-page">
      <div className="prototype-page-header">
        <div>
          <Typography.Title level={3} style={{ margin: 0 }}>
            Skill 管理
          </Typography.Title>
          <Typography.Text type="secondary">
            管理应用内置与用户自定义 Skill，并注入到终端、SQL、日志、SFTP、MCP 等 AI 交互。
          </Typography.Text>
        </div>
      </div>
      <Tabs
        activeKey={activeTab}
        onChange={setActiveTab}
        items={[
          { key: "skills", label: "技能", children: <SkillTab /> },
          { key: "experiences", label: "经验库", children: <ExperienceTab /> },
          { key: "runbooks", label: "Runbook", children: <RunbookTab /> },
        ]}
      />
    </div>
  );
}

function SkillTab() {
  const [loading, setLoading] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [keyword, setKeyword] = useState("");
  const [source, setSource] = useState<"all" | "user" | "builtin">("all");
  const [showBuiltin, setShowBuiltin] = useState(true);
  const [scope, setScope] = useState<AiSkillScope | undefined>();
  const [items, setItems] = useState<AiSkill[]>([]);
  const [stats, setStats] = useState({ total: 0, user: 0, builtin: 0, enabled: 0 });
  const [triggerPrompt, setTriggerPrompt] = useState("");
  const [triggerResult, setTriggerResult] = useState<AiSkillTriggerResult | null>(null);
  const [triggerLoading, setTriggerLoading] = useState(false);
  const [editing, setEditing] = useState<AiSkill | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [form] = Form.useForm<UpsertAiSkillInput>();
  const [preview, setPreview] = useState<AiSkillPromptPreviewResult | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);

  async function load() {
    setLoading(true);
    try {
      const result = await aiSkillApi.list({
        keyword: keyword || null,
        source: source === "all" ? null : source,
        showBuiltin,
        scope: scope ?? null,
      });
      setItems(result.items);
      setStats(result.stats);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load();
  }, [keyword, source, showBuiltin, scope]);

  async function runTriggerTest(showToast = false) {
    if (!triggerPrompt.trim()) {
      setTriggerResult(null);
      if (showToast) {
        message.warning("请输入要测试的 prompt");
      }
      return;
    }
    setTriggerLoading(true);
    try {
      const result = await aiSkillApi.testTrigger({
        prompt: triggerPrompt,
        scope: scope ?? null,
        includeGlobal: true,
      });
      setTriggerResult(result);
      if (showToast) {
        const experiences = result.experiences ?? [];
        const total = result.matches.length + experiences.length;
        message.success(
          total
            ? `命中 ${result.matches.length} 个 Skill，${experiences.length} 条经验`
            : "未命中 Skill 或经验",
        );
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setTriggerLoading(false);
    }
  }

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void runTriggerTest(false);
    }, 500);
    return () => window.clearTimeout(timer);
  }, [triggerPrompt, scope]);

  function openCreate() {
    setEditing(null);
    setPreview(null);
    form.setFieldsValue({
      id: null,
      skillKey: "",
      name: "",
      description: "",
      content: "",
      scopes: ["global"],
      triggerWords: [],
      tags: [],
      priority: 50,
      enabled: true,
      allowMcp: true,
    });
    setDrawerOpen(true);
  }

  function openEdit(skill: AiSkill) {
    setEditing(skill);
    setPreview(null);
    form.setFieldsValue({
      id: skill.id,
      skillKey: skill.skillKey,
      name: skill.name,
      description: skill.description,
      content: skill.content,
      scopes: skill.scopes,
      triggerWords: skill.triggerWords,
      tags: skill.tags,
      priority: skill.priority,
      enabled: skill.enabled,
      allowMcp: skill.allowMcp,
    });
    setDrawerOpen(true);
  }

  async function saveSkill() {
    try {
      const values = await form.validateFields();
      await aiSkillApi.upsert({
        ...values,
        triggerWords: splitTags(values.triggerWords),
        tags: splitTags(values.tags),
      });
      message.success("Skill 已保存");
      setDrawerOpen(false);
      await load();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function syncBuiltin() {
    setSyncing(true);
    try {
      const result = await aiSkillApi.syncBuiltin();
      message.success(`同步完成：扫描 ${result.scanned}，新增 ${result.inserted}，更新 ${result.updated}`);
      await load();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSyncing(false);
    }
  }

  async function previewPrompt() {
    setPreviewLoading(true);
    try {
      const values = form.getFieldsValue();
      setPreview(
        await aiSkillApi.previewPrompt({
          prompt: triggerPrompt || values.description || values.name,
          scope: (values.scopes?.[0] as AiSkillScope | undefined) ?? "global",
          includeGlobal: true,
        }),
      );
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setPreviewLoading(false);
    }
  }

  const matchMap = useMemo(() => {
    const map = new Map<number, number>();
    triggerResult?.matches.forEach((item) => map.set(item.skill.id, item.matchedWords.length));
    return map;
  }, [triggerResult]);

  const columns: ColumnsType<AiSkill> = [
    {
      title: "名称",
      dataIndex: "name",
      width: 230,
      render: (_, record) => (
        <Button type="link" style={{ padding: 0 }} onClick={() => openEdit(record)}>
          {record.name}
        </Button>
      ),
    },
    { title: "来源", width: 110, render: (_, record) => sourceTag(record) },
    {
      title: "描述",
      dataIndex: "description",
      ellipsis: true,
    },
    {
      title: "触发词",
      width: 360,
      render: (_, record) => (
        <Space size={[4, 4]} wrap>
          {record.triggerWords.slice(0, 6).map((word) => (
            <Tag key={word} style={{ whiteSpace: "nowrap" }}>{word}</Tag>
          ))}
          {record.triggerWords.length > 6 ? <Tag>+{record.triggerWords.length - 6}</Tag> : null}
        </Space>
      ),
    },
    {
      title: "命中",
      width: 80,
      render: (_, record) => {
        const count = matchMap.get(record.id);
        return count ? <Badge color="blue" text={count} /> : "-";
      },
    },
    {
      title: "操作",
      width: 240,
      render: (_, record) => (
        <Space size={6}>
          <Button size="small" onClick={() => openEdit(record)}>编辑</Button>
          <Button size="small" icon={<Copy size={14} />} onClick={() => aiSkillApi.copy(record.id).then(load)}>复制</Button>
          <Switch
            size="small"
            checked={record.enabled}
            onChange={(checked) => aiSkillApi.setEnabled(record.id, checked).then(load)}
          />
          {record.builtin ? (
            <Button size="small" icon={<RotateCcw size={14} />} onClick={() => aiSkillApi.restoreBuiltin(record.id).then(load)} />
          ) : (
            <Button
              size="small"
              danger
              icon={<Trash2 size={14} />}
              onClick={() => {
                Modal.confirm({
                  title: "删除 Skill",
                  content: `确认删除 ${record.name}？`,
                  onOk: async () => {
                    await aiSkillApi.delete(record.id);
                    await load();
                  },
                });
              }}
            />
          )}
        </Space>
      ),
    },
  ];

  return (
    <Space direction="vertical" size={12} style={{ width: "100%" }}>
      <Card size="small" title={<Space><Sparkles size={16} />测试触发</Space>}>
        <Space direction="vertical" size={8} style={{ width: "100%" }}>
          <Typography.Text type="secondary">
            输入运维问题后点击测试，系统会按触发词和作用域计算会注入哪些 Skill；输入停止后也会自动预览。
          </Typography.Text>
          <Space.Compact style={{ width: "100%" }}>
            <Input
              value={triggerPrompt}
              onChange={(event) => setTriggerPrompt(event.target.value)}
              onPressEnter={() => runTriggerTest(true)}
              placeholder={'测试："nginx 502 怎么排查" 或 "docker 容器一直 restarting"'}
            />
            <Button loading={triggerLoading} type="primary" onClick={() => runTriggerTest(true)}>
              测试触发
            </Button>
            <Button
              onClick={() => {
                setTriggerPrompt("");
                setTriggerResult(null);
              }}
            >
              清空
            </Button>
          </Space.Compact>
        </Space>
        {triggerResult?.matches.length ? (
          <div className="mt-3">
            <Typography.Text type="secondary">命中：</Typography.Text>
            <Space size={[6, 6]} wrap>
              {triggerResult.matches.map((item) => (
                <Tag color="blue" key={item.skill.id}>{item.skill.name} · {item.matchedWords.length}</Tag>
              ))}
            </Space>
          </div>
        ) : null}
        {(triggerResult?.experiences ?? []).length ? (
          <div className="mt-3">
            <Typography.Text type="secondary">经验库命中：</Typography.Text>
            <Space direction="vertical" size={6} style={{ width: "100%", marginTop: 8 }}>
              {(triggerResult?.experiences ?? []).map((item) => (
                <Card size="small" key={item.experience.id}>
                  <Space direction="vertical" size={4} style={{ width: "100%" }}>
                    <Space wrap>
                      <Tag color="purple">{item.experience.scenario || "未分类"}</Tag>
                      <Typography.Text strong>{item.experience.title}</Typography.Text>
                      <Typography.Text type="secondary">得分 {item.score}</Typography.Text>
                    </Space>
                    <Typography.Paragraph style={{ marginBottom: 0, whiteSpace: "pre-wrap" }}>
                      {item.summary || item.experience.solution.slice(0, 220)}
                    </Typography.Paragraph>
                    <Space size={[4, 4]} wrap>
                      {item.matchedWords.map((word) => <Tag key={word}>{word}</Tag>)}
                    </Space>
                  </Space>
                </Card>
              ))}
            </Space>
          </div>
        ) : null}
        {triggerPrompt.trim() && triggerResult && triggerResult.matches.length === 0 && (triggerResult.experiences ?? []).length === 0 ? (
          <div className="mt-3">
            <Typography.Text type="secondary">未命中 Skill 或经验。可以检查触发词、作用域筛选、经验场景或是否启用。</Typography.Text>
          </div>
        ) : null}
      </Card>

      <Card
        size="small"
        title={<Space><BookOpen size={16} />技能</Space>}
        extra={
          <Space>
            <Button size="small" icon={<RefreshCw size={14} />} loading={syncing} onClick={syncBuiltin}>刷新内置</Button>
            <Button size="small" type="primary" icon={<FilePlus size={14} />} onClick={openCreate}>新建技能</Button>
          </Space>
        }
      >
        <Space wrap style={{ marginBottom: 12 }}>
          <Button size="small" type={source === "all" ? "primary" : "default"} onClick={() => setSource("all")}>全部 {stats.total}</Button>
          <Button size="small" type={source === "user" ? "primary" : "default"} onClick={() => setSource("user")}>用户 {stats.user}</Button>
          <Button size="small" type={source === "builtin" ? "primary" : "default"} onClick={() => setSource("builtin")}>内置 {stats.builtin}</Button>
          <Input
            allowClear
            prefix={<Search size={14} />}
            value={keyword}
            onChange={(event) => setKeyword(event.target.value)}
            placeholder="按名称/描述/触发词搜索"
            style={{ width: 260 }}
          />
          <Select
            allowClear
            placeholder="作用域"
            options={scopeOptions}
            value={scope}
            onChange={setScope}
            style={{ width: 180 }}
          />
          <Space>
            <Switch checked={showBuiltin} onChange={setShowBuiltin} />
            <Typography.Text>显示内置</Typography.Text>
          </Space>
        </Space>
        <Table
          rowKey="id"
          size="small"
          loading={loading}
          columns={columns}
          dataSource={items}
          pagination={{ pageSize: 12, showSizeChanger: true }}
          expandable={{
            expandedRowRender: (record) => (
              <Space direction="vertical" size={8}>
                <div>{scopeTags(record.scopes)}</div>
                <Typography.Paragraph style={{ marginBottom: 0, whiteSpace: "pre-wrap" }}>
                  {record.content.slice(0, 600)}
                  {record.content.length > 600 ? "..." : ""}
                </Typography.Paragraph>
              </Space>
            ),
          }}
        />
      </Card>

      <Drawer
        title={editing ? `编辑 Skill：${editing.name}` : "新建 Skill"}
        width={760}
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        extra={<Space><Button onClick={() => setDrawerOpen(false)}>取消</Button><Button type="primary" onClick={saveSkill}>保存</Button></Space>}
      >
        <Form form={form} layout="vertical">
          <Form.Item name="id" hidden><Input /></Form.Item>
          <Form.Item label="来源">
            <Tag color={editing?.builtin ? "blue" : "green"}>{editing?.builtin ? "内置" : "用户"}</Tag>
            {editing?.builtin ? <Typography.Text type="secondary"> 内置内容可覆盖，不能删除。</Typography.Text> : null}
          </Form.Item>
          <Form.Item name="name" label="名称" rules={[{ required: true, message: "请输入名称" }]}>
            <Input />
          </Form.Item>
          <Form.Item name="skillKey" label="Skill Key">
            <Input disabled={Boolean(editing?.builtin)} placeholder="留空时按名称自动生成" />
          </Form.Item>
          <Form.Item name="description" label="描述">
            <Input />
          </Form.Item>
          <Form.Item name="scopes" label="作用域" rules={[{ required: true, message: "请选择作用域" }]}>
            <Select mode="multiple" options={scopeOptions} />
          </Form.Item>
          <Space style={{ width: "100%" }} size={12} align="start">
            <Form.Item name="priority" label="优先级" style={{ width: 140 }}>
              <InputNumber min={0} max={999} style={{ width: "100%" }} />
            </Form.Item>
            <Form.Item name="enabled" label="启用" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item name="allowMcp" label="允许 MCP 使用" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Space>
          <Form.Item name="triggerWords" label="触发词">
            <Select mode="tags" tokenSeparators={[",", "，", "、"]} open={false} placeholder="输入后回车" />
          </Form.Item>
          <Form.Item name="tags" label="标签">
            <Select mode="tags" tokenSeparators={[",", "，", "、"]} open={false} placeholder="输入后回车" />
          </Form.Item>
          <Form.Item name="content" label="Skill 内容" rules={[{ required: true, message: "请输入 Skill 内容" }]}>
            <Input.TextArea rows={14} style={{ fontFamily: "var(--font-mono)" }} />
          </Form.Item>
          <Collapse
            items={[
              {
                key: "preview",
                label: "Prompt 预览",
                children: (
                  <Space direction="vertical" style={{ width: "100%" }}>
                    <Button loading={previewLoading} onClick={previewPrompt}>生成预览</Button>
                    <Input.TextArea
                      rows={8}
                      readOnly
                      value={preview?.promptFragment ?? ""}
                      placeholder="预览不包含服务器密码、API Key、凭证明文。"
                    />
                  </Space>
                ),
              },
            ]}
          />
        </Form>
      </Drawer>
    </Space>
  );
}

function ExperienceTab() {
  const [keyword, setKeyword] = useState("");
  const [items, setItems] = useState<AiExperience[]>([]);
  const [loading, setLoading] = useState(false);
  const [open, setOpen] = useState(false);
  const [form] = Form.useForm<UpsertAiExperienceInput>();

  async function load() {
    setLoading(true);
    try {
      setItems(await aiSkillApi.listExperiences(keyword));
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load();
  }, [keyword]);

  function edit(record?: AiExperience) {
    form.setFieldsValue(record ?? { title: "", symptom: "", cause: "", solution: "", scenario: "", source: "user", tags: [], enabled: true });
    setOpen(true);
  }

  async function save() {
    try {
      await aiSkillApi.upsertExperience(await form.validateFields());
      message.success("经验已保存");
      setOpen(false);
      await load();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  return (
    <Card
      title="经验库"
      extra={<Space><Input allowClear prefix={<Search size={14} />} value={keyword} onChange={(e) => setKeyword(e.target.value)} placeholder="搜经验" /><Button type="primary" onClick={() => edit()}>新建</Button></Space>}
    >
      <Table
        rowKey="id"
        loading={loading}
        dataSource={items}
        locale={{ emptyText: <Empty description="还没有经验沉淀。AI 经 MCP recall_experience 工具会查这里；先写几条最近的踩坑记。" /> }}
        columns={[
          { title: "标题", dataIndex: "title" },
          { title: "场景", dataIndex: "scenario", width: 160 },
          { title: "来源", dataIndex: "source", width: 100 },
          { title: "标签", width: 220, render: (_, record) => record.tags.map((tag) => <Tag key={tag}>{tag}</Tag>) },
          { title: "更新时间", dataIndex: "updatedAt", width: 170 },
          {
            title: "操作",
            width: 120,
            render: (_, record) => (
              <Space>
                <Button size="small" onClick={() => edit(record)}>编辑</Button>
                <Button size="small" danger onClick={() => aiSkillApi.deleteExperience(record.id).then(load)}>删除</Button>
              </Space>
            ),
          },
        ]}
      />
      <Drawer title="经验" width={680} open={open} onClose={() => setOpen(false)} extra={<Button type="primary" onClick={save}>保存</Button>}>
        <Form form={form} layout="vertical">
          <Form.Item name="id" hidden><Input /></Form.Item>
          <Form.Item name="title" label="标题" rules={[{ required: true }]}><Input /></Form.Item>
          <Form.Item name="scenario" label="适用场景"><Input /></Form.Item>
          <Form.Item name="symptom" label="问题现象"><Input.TextArea rows={3} /></Form.Item>
          <Form.Item name="cause" label="根因"><Input.TextArea rows={3} /></Form.Item>
          <Form.Item name="solution" label="解决方案"><Input.TextArea rows={5} /></Form.Item>
          <Form.Item name="tags" label="标签"><Select mode="tags" open={false} /></Form.Item>
          <Form.Item name="enabled" label="启用" valuePropName="checked"><Switch /></Form.Item>
        </Form>
      </Drawer>
    </Card>
  );
}

function RunbookTab() {
  const [keyword, setKeyword] = useState("");
  const [items, setItems] = useState<AiRunbook[]>([]);
  const [loading, setLoading] = useState(false);
  const [open, setOpen] = useState(false);
  const [runningId, setRunningId] = useState<number | null>(null);
  const [runResult, setRunResult] = useState<AiRunbookRunResult | null>(null);
  const [runModalOpen, setRunModalOpen] = useState(false);
  const [form] = Form.useForm<UpsertAiRunbookInput>();

  async function load() {
    setLoading(true);
    try {
      setItems(await aiSkillApi.listRunbooks(keyword));
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load();
  }, [keyword]);

  function edit(record?: AiRunbook) {
    form.setFieldsValue(
      record ?? {
        name: "",
        description: "",
        scenario: "",
        tags: [],
        steps: [{ id: crypto.randomUUID(), title: "", stepType: "note", content: "", riskLevel: "low" }],
        enabled: true,
        allowMcp: false,
      },
    );
    setOpen(true);
  }

  async function save() {
    try {
      await aiSkillApi.upsertRunbook(await form.validateFields());
      message.success("Runbook 已保存");
      setOpen(false);
      await load();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function run(record: AiRunbook, dryRun: boolean) {
    setRunningId(record.id);
    try {
      const result = await aiSkillApi.runRunbook({
        id: record.id,
        requester: "local-user",
        dryRun,
      });
      setRunResult(result);
      setRunModalOpen(true);
      message.success(result.message);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setRunningId(null);
    }
  }

  return (
    <Card
      title="Runbook"
      extra={<Space><Input allowClear prefix={<Search size={14} />} value={keyword} onChange={(e) => setKeyword(e.target.value)} placeholder="搜 Runbook" /><Button type="primary" onClick={() => edit()}>新建</Button></Space>}
    >
      <Table
        rowKey="id"
        loading={loading}
        dataSource={items}
        locale={{ emptyText: <Empty description="还没有固化的多步操作。可在本页执行，也可通过 MCP run_runbook 调用。" /> }}
        columns={[
          { title: "名称", dataIndex: "name" },
          { title: "场景", dataIndex: "scenario", width: 160 },
          { title: "步骤", width: 90, render: (_, record) => record.steps.length },
          { title: "允许 MCP", width: 100, render: (_, record) => (record.allowMcp ? "是" : "否") },
          { title: "更新时间", dataIndex: "updatedAt", width: 170 },
          {
            title: "操作",
            width: 240,
            render: (_, record) => (
              <Space>
                <Button size="small" loading={runningId === record.id} onClick={() => run(record, true)}>预演</Button>
                <Button size="small" type="primary" loading={runningId === record.id} onClick={() => run(record, false)}>执行</Button>
                <Button size="small" onClick={() => edit(record)}>编辑</Button>
                <Button size="small" danger onClick={() => aiSkillApi.deleteRunbook(record.id).then(load)}>删除</Button>
              </Space>
            ),
          },
        ]}
      />
      <Modal
        title="Runbook 执行结果"
        width={920}
        open={runModalOpen}
        onCancel={() => setRunModalOpen(false)}
        footer={<Button type="primary" onClick={() => setRunModalOpen(false)}>关闭</Button>}
      >
        {runResult ? (
          <Space direction="vertical" size={12} style={{ width: "100%" }}>
            <Space wrap>
              <Typography.Text strong>{runResult.runbook.name}</Typography.Text>
              {runbookStatusTag(runResult.status)}
              <Typography.Text type="secondary">{runResult.message}</Typography.Text>
            </Space>
            <Table
              size="small"
              rowKey="stepId"
              pagination={false}
              dataSource={runResult.steps}
              expandable={{
                expandedRowRender: (record) => (
                  <pre className="max-h-80 overflow-auto rounded border border-[var(--border-color)] bg-[var(--bg-tertiary)] p-3 text-xs whitespace-pre-wrap">
                    {formatRunbookOutput(record.output) || "无输出"}
                  </pre>
                ),
              }}
              columns={[
                { title: "步骤", dataIndex: "title", width: 180 },
                { title: "类型", dataIndex: "stepType", width: 130 },
                { title: "风险", dataIndex: "riskLevel", width: 90 },
                { title: "状态", dataIndex: "status", width: 100, render: (status) => runbookStatusTag(status) },
                { title: "说明", dataIndex: "message" },
                { title: "审批 ID", dataIndex: "approvalId", width: 90, render: (value) => value ?? "-" },
                { title: "耗时", dataIndex: "durationMs", width: 90, render: (value) => `${value} ms` },
              ]}
            />
          </Space>
        ) : (
          <Empty description="暂无执行结果" />
        )}
      </Modal>
      <Drawer title="Runbook" width={760} open={open} onClose={() => setOpen(false)} extra={<Button type="primary" onClick={save}>保存</Button>}>
        <Form form={form} layout="vertical">
          <Form.Item name="id" hidden><Input /></Form.Item>
          <Form.Item name="name" label="名称" rules={[{ required: true }]}><Input /></Form.Item>
          <Form.Item name="description" label="描述"><Input /></Form.Item>
          <Form.Item name="scenario" label="适用场景"><Input /></Form.Item>
          <Form.Item name="tags" label="标签"><Select mode="tags" open={false} /></Form.Item>
          <Space>
            <Form.Item name="enabled" label="启用" valuePropName="checked"><Switch /></Form.Item>
            <Form.Item name="allowMcp" label="允许 MCP 调用" valuePropName="checked"><Switch /></Form.Item>
          </Space>
          <Form.List name="steps">
            {(fields, { add, remove }) => (
              <Space direction="vertical" style={{ width: "100%" }}>
                <Button onClick={() => add({ id: crypto.randomUUID(), title: "", stepType: "note", content: "", riskLevel: "low" } satisfies AiRunbookStep)}>
                  添加步骤
                </Button>
                {fields.map((field, index) => (
                  <Card size="small" key={field.key} title={`步骤 ${index + 1}`} extra={<Button danger size="small" onClick={() => remove(field.name)}>删除</Button>}>
                    <Form.Item name={[field.name, "id"]} hidden><Input /></Form.Item>
                    <Form.Item name={[field.name, "title"]} label="标题" rules={[{ required: true }]}><Input /></Form.Item>
                    <Space style={{ width: "100%" }} align="start">
                      <Form.Item name={[field.name, "stepType"]} label="类型" style={{ width: 180 }}><Select options={stepTypeOptions} /></Form.Item>
                      <Form.Item name={[field.name, "riskLevel"]} label="风险" style={{ width: 140 }}><Select options={riskOptions} /></Form.Item>
                    </Space>
                    <Form.Item name={[field.name, "content"]} label="内容"><Input.TextArea rows={4} /></Form.Item>
                  </Card>
                ))}
              </Space>
            )}
          </Form.List>
        </Form>
      </Drawer>
    </Card>
  );
}
