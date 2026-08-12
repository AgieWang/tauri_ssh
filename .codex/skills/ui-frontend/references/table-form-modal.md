# Ant Design 表格、表单与弹窗模式

## 目录

1. 页面状态
2. Table
3. Form/Modal
4. CRUD
5. 常见错误

## 1. 页面状态

一个后端数据页面至少区分：

- 首次 loading；
- 空数据；
- 可重试 error；
- 有数据；
- 写操作提交中；
- 写成功后的刷新/局部更新。

```tsx
const [data, setData] = useState<Item[]>([]);
const [loading, setLoading] = useState(false);
const [error, setError] = useState<string | null>(null);

async function loadData() {
  setLoading(true);
  setError(null);
  try {
    setData(await itemApi.list());
  } catch (cause: unknown) {
    const text = getErrorMessage(cause);
    setError(text);
    message.error(text);
  } finally {
    setLoading(false);
  }
}
```

useEffect 内异步加载要处理 StrictMode 重复执行、请求竞态和卸载；优先复用仓库已有 hook/取消模式。

## 2. Table

```tsx
const columns: ColumnsType<Item> = [
  { title: "名称", dataIndex: "name", key: "name" },
  { title: "状态", dataIndex: "status", key: "status" },
  {
    title: "操作",
    key: "actions",
    render: (_, record) => (
      <Space>
        <Button onClick={() => openEdit(record)}>编辑</Button>
        <Popconfirm title="确认删除？" onConfirm={() => remove(record.id)}>
          <Button danger>删除</Button>
        </Popconfirm>
      </Space>
    ),
  },
];

<Table<Item>
  rowKey="id"
  columns={columns}
  dataSource={data}
  loading={loading}
  locale={{ emptyText: error ? <Alert type="error" message={error} /> : "暂无数据" }}
/>
```

- `rowKey` 稳定且来自业务主键，不使用数组 index。
- 分页、筛选和排序由真实数据量/后端能力决定。
- 操作按钮具备文本或可访问名称；仅图标按钮提供 `aria-label`/Tooltip。
- 大量列考虑横向滚动和固定操作列，验证小窗口。

## 3. Form 与 Modal

```tsx
interface ItemFormValues {
  name: string;
  description?: string;
}

const [form] = Form.useForm<ItemFormValues>();
const [submitting, setSubmitting] = useState(false);

async function submit(values: ItemFormValues) {
  setSubmitting(true);
  try {
    await itemApi.create(values);
    message.success("创建成功");
    setOpen(false);
    form.resetFields();
    await loadData();
  } catch (error: unknown) {
    message.error(getErrorMessage(error));
  } finally {
    setSubmitting(false);
  }
}

<Modal
  title="新增项目"
  open={open}
  confirmLoading={submitting}
  onOk={() => form.submit()}
  onCancel={() => !submitting && setOpen(false)}
  destroyOnHidden
  mask={{ closable: false }}
>
  <Form form={form} layout="vertical" onFinish={submit} preserve={false}>
    <Form.Item
      name="name"
      label="名称"
      rules={[{ required: true, whitespace: true, message: "请输入名称" }]}
    >
      <Input maxLength={100} autoFocus />
    </Form.Item>
    <Form.Item name="description" label="描述">
      <Input.TextArea maxLength={500} showCount />
    </Form.Item>
  </Form>
</Modal>
```

- 创建和编辑的初始值、reset 时机要分别验证。
- 提交中禁用取消/重复提交是否符合业务；不可无条件关闭导致结果丢失。
- 服务端字段错误若可映射，使用 `form.setFields` 显示在对应字段。
- Modal 关闭后焦点应返回触发按钮；确认 Esc/Tab/Enter 行为。
- 含输入或多步操作的 Modal/Drawer 在 Ant Design 6 设置 `mask={{ closable: false }}`，防止误点遮罩丢失内容；纯查看或无状态命令浮层保留默认行为。
- `maskClosable` 与 `destroyOnClose` 已弃用；不要与 `mask.closable`、`destroyOnHidden` 同时声明两套配置。以后升级仍以当前类型定义为准。

## 4. CRUD 流程

1. 首次加载显示 Skeleton/Table loading。
2. 新增/编辑打开 Modal，字段有 label、限制和错误。
3. 保存成功后根据数据规模选择局部更新或重新查询；不要同时做两者导致闪烁。
4. 删除使用 `Popconfirm`/`Modal.confirm`，提交中锁定重复动作。
5. 删除失败保留列表项并展示可理解错误。
6. 列表刷新保留用户合理的分页/筛选状态。

## 5. 常见错误

| 错误 | 修正 |
|---|---|
| 原生 table/form/button 重写 antd 能力 | 优先 Ant Design 组件 |
| 组件内裸写 invoke | 使用业务 API 模块 |
| `message.error(String(error))` | 使用 `getErrorMessage(error)` |
| loading 时仍可重复提交 | `confirmLoading`/disabled + 幂等控制 |
| 删除用 `confirm()` | 使用 antd `Popconfirm`/`Modal.confirm` |
| 颜色写死或 `dark:` 双轨 | CSS Variables / antd token |
| 用 index 当 rowKey | 使用稳定业务 ID |
| 只有成功路径 | 补齐空态、错误、重试和取消 |
