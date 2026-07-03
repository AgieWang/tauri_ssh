import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Alert, Badge, Button, Card, Checkbox, Descriptions, Divider, Drawer, Form, Input, InputNumber, Modal, Popconfirm, Progress, Radio, Select, Space, Steps, Switch, Table, Tabs, Tag, Tooltip, Typography, message } from "antd";
import type { TableProps } from "antd";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { ArrowDownToLine, Bot, ChevronLeft, ChevronRight, ChevronsUp, Copy, Edit3, Eye, FilePlus2, Folder, FolderOpen, FolderPlus, Home, KeyRound, Link2, Maximize2, Minimize2, Pencil, PlugZap, Plus, RefreshCw, Scissors, Search, ShieldAlert, Trash2, Upload, UploadCloud } from "lucide-react";
import {
  coverageRows,
  servers,
  type CoverageRecord,
  type ServerRecord,
} from "@/data/prototype";
import { AiInsightPanel, CodeBlock, PageHeader, RiskBadge, SectionGrid, StatCard, TwoColumn } from "@/components/prototype/common";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";
import { aiProviderApi, aiSkillApi, approvalApi, auditApi, credentialVaultApi, getErrorMessage, hasTauriRuntime, jumpserverApi, mcpApi, sftpApi, sshServerApi, systemSettingsApi, terminalApi } from "@/lib/api";
import { useAppStore } from "@/store";
import type { AiProvider, AiProviderModelListInput, AiProviderRegion, ApprovalRequest, ApprovalStatus, AuditLog, AuditRisk, CreateApprovalRequestInput, CredentialStatus, CredentialType, CredentialVaultItem, JumpServerAiMode, JumpServerProtocol, JumpServerSession, JumpServerStatus, ListAuditLogsInput, McpClientConfig, McpOverview, SftpFileEntry, SshServer, SshServerAuthType, SshServerPolicy, SshServerSource, SystemSettings, TerminalCommandResult, TerminalSessionEvent, UpsertCredentialInput, UpsertJumpServerSessionInput, UpsertSshServerInput } from "@/types";

const { Paragraph, Text, Title } = Typography;
const ALL_PROVIDER_TEST_KEY = "__all_configured_providers__";
const TERMINAL_BOTTOM_RESERVED_ROWS = 2;

const SftpCodeEditor = lazy(async () => {
  const [
    codeMirrorModule,
    githubThemeModule,
    commandsModule,
    cssModule,
    cppModule,
    goModule,
    htmlModule,
    javaModule,
    javascriptModule,
    jsonModule,
    markdownModule,
    phpModule,
    pythonModule,
    rustModule,
    sqlModule,
    xmlModule,
    yamlModule,
    languageModule,
    viewModule,
  ] = await Promise.all([
    import("@uiw/react-codemirror"),
    import("@uiw/codemirror-theme-github"),
    import("@codemirror/commands"),
    import("@codemirror/lang-css"),
    import("@codemirror/lang-cpp"),
    import("@codemirror/lang-go"),
    import("@codemirror/lang-html"),
    import("@codemirror/lang-java"),
    import("@codemirror/lang-javascript"),
    import("@codemirror/lang-json"),
    import("@codemirror/lang-markdown"),
    import("@codemirror/lang-php"),
    import("@codemirror/lang-python"),
    import("@codemirror/lang-rust"),
    import("@codemirror/lang-sql"),
    import("@codemirror/lang-xml"),
    import("@codemirror/lang-yaml"),
    import("@codemirror/language"),
    import("@codemirror/view"),
  ]);
  const CodeMirror = codeMirrorModule.default;
  const resolveLanguageExtensions = (languageKey: string) => {
    if (languageKey === "shell") return [javascriptModule.javascript()];
    if (languageKey === "javascript") return [javascriptModule.javascript({ jsx: true })];
    if (languageKey === "typescript") return [javascriptModule.javascript({ jsx: true, typescript: true })];
    if (languageKey === "json") return [jsonModule.json()];
    if (languageKey === "yaml") return [yamlModule.yaml()];
    if (languageKey === "html") return [htmlModule.html()];
    if (languageKey === "css") return [cssModule.css()];
    if (languageKey === "xml") return [xmlModule.xml()];
    if (languageKey === "markdown") return [markdownModule.markdown()];
    if (languageKey === "sql") return [sqlModule.sql()];
    if (languageKey === "python") return [pythonModule.python()];
    if (languageKey === "java") return [javaModule.java()];
    if (languageKey === "rust") return [rustModule.rust()];
    if (languageKey === "go") return [goModule.go()];
    if (languageKey === "php") return [phpModule.php()];
    if (languageKey === "cpp") return [cppModule.cpp()];
    return [];
  };
  return {
    default: function LazySftpCodeEditor(props: { value: string; languageKey: string; onChange: (value: string) => void }) {
      return (
        <CodeMirror
          value={props.value}
          height="calc(100vh - 170px)"
          theme={githubThemeModule.githubLight}
          extensions={[
            languageModule.indentUnit.of("  "),
            viewModule.keymap.of([commandsModule.indentWithTab]),
            ...resolveLanguageExtensions(props.languageKey),
          ]}
          basicSetup={{
            lineNumbers: true,
            highlightActiveLineGutter: true,
            highlightSpecialChars: true,
            foldGutter: true,
            drawSelection: true,
            dropCursor: true,
            allowMultipleSelections: true,
            indentOnInput: true,
            bracketMatching: true,
            closeBrackets: true,
            autocompletion: true,
            rectangularSelection: true,
            crosshairCursor: true,
            highlightActiveLine: true,
            highlightSelectionMatches: true,
            closeBracketsKeymap: true,
            searchKeymap: true,
            foldKeymap: true,
            completionKeymap: true,
            lintKeymap: true,
          }}
          onChange={(value) => props.onChange(value)}
        />
      );
    },
  };
});

const providerStatusMeta: Record<AiProvider["status"], { text: string; color: string; status: "success" | "processing" | "default" | "warning" }> = {
  configured: { text: "已配置", color: "green", status: "success" },
  testing: { text: "待测试", color: "orange", status: "processing" },
  unconfigured: { text: "未配置", color: "default", status: "default" },
  reserved: { text: "预留", color: "blue", status: "warning" },
};

const providerRegionLabel: Record<AiProviderRegion | "all", string> = {
  all: "全部",
  global: "国际",
  china: "国内",
  gateway: "聚合/兼容",
  local: "本地",
};

const sshServerStatusMeta: Record<SshServer["status"], { text: string; color: string; status: "success" | "processing" | "default" | "warning" | "error" }> = {
  unknown: { text: "未检测", color: "default", status: "default" },
  online: { text: "在线", color: "green", status: "success" },
  offline: { text: "离线", color: "red", status: "error" },
  degraded: { text: "异常", color: "orange", status: "warning" },
  web: { text: "网页登录", color: "blue", status: "processing" },
};

const sshServerSourceLabel: Record<SshServerSource, string> = {
  manual: "手工维护",
  ssh_config: "SSH Config",
  jumpserver: "JumpServer",
};

const sshServerAuthTypeLabel: Record<SshServerAuthType, string> = {
  key: "密钥文件",
  password_ref: "凭据保险库",
  direct_password: "直接密码",
  session_reference: "会话引用",
};

const sshPolicyOptions: Array<{ value: SshServerPolicy; label: string }> = [
  { value: "readonly", label: "只读 - 仅允许查看" },
  { value: "L1", label: "低风险 - 只读与安全检查" },
  { value: "L2", label: "中风险 - 常规运维需审批" },
  { value: "L3", label: "高风险 - 变更/重启强审批" },
  { value: "blocked", label: "禁用 - AI 不可操作" },
];

const sshPolicyLabel = Object.fromEntries(
  sshPolicyOptions.map((item) => [item.value, item.label]),
) as Record<SshServerPolicy, string>;

const sshPolicyColor: Record<SshServerPolicy, string> = {
  readonly: "cyan",
  L1: "green",
  L2: "orange",
  L3: "red",
  blocked: "red",
};

function formatSshServerSource(value: SshServerSource | string) {
  return sshServerSourceLabel[value as SshServerSource] ?? (value || "-");
}

function formatSshServerAuth(record: SshServer) {
  if (record.hasPassword) {
    return "已保存密码";
  }
  const authType = sshServerAuthTypeLabel[record.authType] ?? record.authType;
  const authRef = record.authRef || "";
  if (!authRef) {
    return authType;
  }
  if (authRef.startsWith("vault:")) {
    return `凭据保险库：${authRef.replace(/^vault:/, "")}`;
  }
  if (authRef.startsWith("key:")) {
    return `密钥文件：${authRef.replace(/^key:/, "")}`;
  }
  if (authRef.startsWith("password:")) {
    return "已保存密码";
  }
  if (authRef.startsWith("session:")) {
    return `会话引用：${authRef.replace(/^session:/, "")}`;
  }
  return `${authType}：${authRef}`;
}

type LogWatchStatus = "tailing" | "paused" | "error";

interface LogWatchTabState {
  id: string;
  title: string;
  serverAlias: string;
  filePath: string;
  lineCount: number;
  intervalSecs: number;
  keyword: string;
  onlyMatches: boolean;
  regex: boolean;
  caseSensitive: boolean;
  inverse: boolean;
  status: LogWatchStatus;
  raw: string;
  lines: string[];
  error: string | null;
  lastUpdatedAt: string | null;
  lastRunAt: number;
  refreshing: boolean;
}

function formatServerAddress(server: SshServer) {
  return server.source === "jumpserver" ? server.host : `${server.host}:${server.port}`;
}

const credentialTypeOptions: Array<{ value: CredentialType; label: string }> = [
  { value: "private_key", label: "私钥" },
  { value: "password", label: "密码" },
  { value: "token", label: "Token" },
  { value: "api_key", label: "API Key" },
  { value: "session_reference", label: "会话引用" },
];

const credentialTypeLabel = Object.fromEntries(
  credentialTypeOptions.map((item) => [item.value, item.label]),
) as Record<CredentialType, string>;

const credentialStatusMeta: Record<CredentialStatus, { text: string; color: string }> = {
  normal: { text: "正常", color: "green" },
  rotation_due: { text: "建议轮换", color: "orange" },
  session_reference: { text: "非明文凭据", color: "gold" },
  disabled: { text: "已禁用", color: "default" },
};

function formatCredentialRotatedAt(value: string | null) {
  if (!value) {
    return "未轮换";
  }
  const normalized = value.includes("T") ? value : value.replace(" ", "T");
  const date = new Date(normalized);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  const diffMs = Date.now() - date.getTime();
  const diffDays = Math.floor(diffMs / 86_400_000);
  if (diffDays <= 0) {
    return `今日 ${date.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}`;
  }
  return `${diffDays} 天前`;
}

function joinSftpPath(parent: string, name: string) {
  const cleanName = name.trim().replace(/^\/+/, "");
  if (!cleanName) {
    return parent || ".";
  }
  if (!parent || parent === ".") {
    return cleanName;
  }
  if (parent === "/") {
    return `/${cleanName}`;
  }
  return `${parent.replace(/\/+$/, "")}/${cleanName}`;
}

function formatSftpSize(value: number) {
  if (value < 1024) {
    return `${value} B`;
  }
  const units = ["KB", "MB", "GB", "TB"];
  let size = value / 1024;
  let index = 0;
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }
  return `${size.toFixed(size >= 10 ? 1 : 2)} ${units[index]}`;
}

function formatSftpModifiedAt(value: number | null) {
  if (!value) {
    return "-";
  }
  return new Date(value * 1000).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).replace(/\//g, "-");
}

function getSftpEditorLanguage(path: string) {
  const lower = path.toLowerCase();
  const fileName = lower.split("/").filter(Boolean).pop() ?? lower;
  const ext = fileName.includes(".") ? fileName.split(".").pop() ?? "" : "";
  if (["bashrc", "bash_profile", "bash_history", "zshrc", "profile", "env"].includes(fileName) || ["sh", "bash", "zsh", "env"].includes(ext)) {
    return { label: "Shell", key: "shell" };
  }
  if (["js", "jsx", "mjs", "cjs", "ts", "tsx"].includes(ext)) {
    return { label: ext.startsWith("t") ? "TypeScript" : "JavaScript", key: ext.startsWith("t") ? "typescript" : "javascript" };
  }
  if (ext === "json" || fileName.endsWith(".jsonc")) {
    return { label: "JSON", key: "json" };
  }
  if (["yml", "yaml"].includes(ext)) {
    return { label: "YAML", key: "yaml" };
  }
  if (["html", "htm", "vue", "svelte"].includes(ext)) {
    return { label: "HTML", key: "html" };
  }
  if (["css", "scss", "less"].includes(ext)) {
    return { label: "CSS", key: "css" };
  }
  if (["xml", "svg", "plist"].includes(ext)) {
    return { label: "XML", key: "xml" };
  }
  if (["md", "markdown"].includes(ext)) {
    return { label: "Markdown", key: "markdown" };
  }
  if (["sql"].includes(ext)) {
    return { label: "SQL", key: "sql" };
  }
  if (["py", "pyw"].includes(ext)) {
    return { label: "Python", key: "python" };
  }
  if (["java"].includes(ext)) {
    return { label: "Java", key: "java" };
  }
  if (["rs"].includes(ext)) {
    return { label: "Rust", key: "rust" };
  }
  if (["go"].includes(ext)) {
    return { label: "Go", key: "go" };
  }
  if (["php"].includes(ext)) {
    return { label: "PHP", key: "php" };
  }
  if (["c", "cc", "cpp", "cxx", "h", "hpp"].includes(ext)) {
    return { label: "C/C++", key: "cpp" };
  }
  return { label: "Plain Text", key: "text" };
}

type TerminalRiskLevel = "safe" | "review" | "blocked";
type AiCommandRisk = "readonly" | "review" | "high" | "blocked";
type AiPolicyAction = "auto" | "review" | "blocked";

interface AiCommandPlanItem {
  command: string;
  purpose: string;
  risk?: AiCommandRisk;
  readonly?: boolean;
}

interface ClassifiedCommand {
  command: string;
  purpose: string;
  risk: AiCommandRisk;
  reason: string;
}

interface AiCommandExecution {
  plan: ClassifiedCommand;
  result: TerminalCommandResult;
}

interface AiPolicyDecision {
  action: AiPolicyAction;
  reason: string;
}

interface TerminalAiMessage {
  role: "user" | "assistant";
  content: string;
  createdAt: string;
}

interface TerminalTabState {
  id: string;
  title: string;
  serverAlias: string;
  status: string;
  connected: boolean;
  connecting: boolean;
  risk: TerminalRiskLevel;
  transcript: string[];
  aiMessages: TerminalAiMessage[];
}

interface TerminalContext {
  terminal: Terminal;
  fitAddon: FitAddon;
  sessionId: string | null;
  websocket: WebSocket | null;
  connected: boolean;
  inputBuffer: string;
  inputCursorIndex: number;
  aiLineMode: boolean;
  aiBusy: boolean;
  dataDisposable: { dispose: () => void };
  resizeObserver: ResizeObserver;
}

const terminalWorkspace = {
  tabs: [] as TerminalTabState[],
  activeId: undefined as string | undefined,
  contexts: new Map<string, TerminalContext>(),
  hosts: new Map<string, HTMLDivElement>(),
  seq: 0,
  handledRequestKey: null as string | null,
};

function startsWithChinese(value: string) {
  return /^[\u3400-\u4DBF\u4E00-\u9FFF]/u.test(value.trimStart());
}

function truncateText(value: string, maxLength: number) {
  if (value.length <= maxLength) {
    return value;
  }
  return `${value.slice(0, maxLength)}\n...已截断 ${value.length - maxLength} 字符`;
}

function sanitizeTerminalText(value: string) {
  return value.replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, "");
}

function normalizeTerminalProse(value: string) {
  return sanitizeTerminalText(value).replace(/[ \t]{2,}/g, " ").trim();
}

function textCellWidth(value: string) {
  return Array.from(value).reduce((width, char) => {
    if (/[\u0300-\u036F]/u.test(char)) {
      return width;
    }
    if (/[\u1100-\u115F\u2329\u232A\u2E80-\uA4CF\uAC00-\uD7A3\uF900-\uFAFF\uFE10-\uFE19\uFE30-\uFE6F\uFF00-\uFF60\uFFE0-\uFFE6]/u.test(char)) {
      return width + 2;
    }
    return width + 1;
  }, 0);
}

function wrapTerminalText(value: string, maxWidth: number) {
  const lines: string[] = [];
  let current = "";
  let currentWidth = 0;
  Array.from(value).forEach((char) => {
    const charWidth = textCellWidth(char);
    if (current && currentWidth + charWidth > maxWidth) {
      lines.push(current.trimEnd());
      current = char.trimStart();
      currentWidth = textCellWidth(current);
      return;
    }
    current += char;
    currentWidth += charWidth;
  });
  if (current || lines.length === 0) {
    lines.push(current.trimEnd());
  }
  return lines;
}

function writeWrappedTerminalLine(
  terminal: Terminal,
  value: string,
  options: {
    color?: string;
    firstPrefix?: string;
    nextPrefix?: string;
    preserveSpaces?: boolean;
    formatter?: (line: string) => string;
  } = {},
) {
  const firstPrefix = options.firstPrefix ?? "";
  const nextPrefix = options.nextPrefix ?? firstPrefix;
  const source = options.preserveSpaces ? sanitizeTerminalText(value).trimEnd() : normalizeTerminalProse(value);
  const color = options.color ?? "";
  const reset = color ? "\x1b[0m" : "";
  const maxFirstWidth = Math.max(24, terminal.cols - textCellWidth(firstPrefix) - 4);
  const maxNextWidth = Math.max(24, terminal.cols - textCellWidth(nextPrefix) - 4);
  const firstWrap = wrapTerminalText(source, maxFirstWidth);
  firstWrap.forEach((line, index) => {
    const prefix = index === 0 ? firstPrefix : nextPrefix;
    const maxWidth = index === 0 ? maxFirstWidth : maxNextWidth;
    const wrappedLines = index === 0 ? [line] : wrapTerminalText(line, maxWidth);
    wrappedLines.forEach((wrappedLine) => {
      terminal.writeln(`${color}${prefix}${options.formatter ? options.formatter(wrappedLine) : wrappedLine}${reset}`);
    });
  });
}

function writeWrappedTerminalBlock(terminal: Terminal, value: string, color?: string) {
  value.replace(/\r\n/g, "\n").split("\n").forEach((line) => {
    if (!line.trim()) {
      terminal.writeln("");
      return;
    }
    writeWrappedTerminalLine(terminal, line, { color });
  });
}

function formatMarkdownInline(value: string) {
  return sanitizeTerminalText(value)
    .replace(/`([^`]+)`/g, "\x1b[96m$1\x1b[0m")
    .replace(/\*\*([^*]+)\*\*/g, "\x1b[1m$1\x1b[0m")
    .replace(/__([^_]+)__/g, "\x1b[1m$1\x1b[0m")
    .replace(/\*([^*\n]+)\*/g, "\x1b[3m$1\x1b[0m")
    .replace(/_([^_\n]+)_/g, "\x1b[3m$1\x1b[0m");
}

function writeMarkdownToTerminal(terminal: Terminal, markdown: string) {
  const lines = markdown.replace(/\r\n/g, "\n").split("\n");
  let inCodeBlock = false;
  lines.forEach((rawLine) => {
    const line = rawLine.trimEnd();
    if (/^```/.test(line.trim())) {
      inCodeBlock = !inCodeBlock;
      terminal.writeln(inCodeBlock ? "\x1b[90m┌─ code\x1b[0m" : "\x1b[90m└─\x1b[0m");
      return;
    }
    if (inCodeBlock) {
      writeWrappedTerminalLine(terminal, line, { firstPrefix: "│ ", nextPrefix: "│ ", preserveSpaces: true, color: "\x1b[90m" });
      return;
    }
    if (!line.trim()) {
      terminal.writeln("");
      return;
    }
    const heading = line.match(/^(#{1,6})\s+(.+)$/);
    if (heading) {
      const level = heading[1].length;
      const prefix = level <= 2 ? "■" : "◆";
      writeWrappedTerminalLine(terminal, heading[2], {
        color: "\x1b[1;36m",
        firstPrefix: `${prefix} `,
        nextPrefix: "  ",
        formatter: formatMarkdownInline,
      });
      return;
    }
    const unordered = line.match(/^(\s*)[-*+]\s+(.+)$/);
    if (unordered) {
      writeWrappedTerminalLine(terminal, unordered[2], {
        firstPrefix: "  • ",
        nextPrefix: "    ",
        formatter: formatMarkdownInline,
      });
      return;
    }
    const ordered = line.match(/^(\s*)(\d+)[.)]\s+(.+)$/);
    if (ordered) {
      const firstPrefix = `${ordered[2]}. `;
      writeWrappedTerminalLine(terminal, ordered[3], {
        color: "\x1b[33m",
        firstPrefix,
        nextPrefix: " ".repeat(textCellWidth(firstPrefix)),
        formatter: formatMarkdownInline,
      });
      return;
    }
    const quote = line.match(/^>\s*(.+)$/);
    if (quote) {
      writeWrappedTerminalLine(terminal, quote[1], {
        color: "\x1b[90m",
        firstPrefix: "│ ",
        nextPrefix: "│ ",
        formatter: formatMarkdownInline,
      });
      return;
    }
    if (/^---+$/.test(line.trim())) {
      terminal.writeln("\x1b[90m────────────────────────────────────────\x1b[0m");
      return;
    }
    writeWrappedTerminalLine(terminal, line, { formatter: formatMarkdownInline });
  });
}

function normalizeMarkdownForPanel(markdown: string) {
  return markdown
    .replace(/\s+(#{1,6}\s+)/g, "\n\n$1")
    .replace(/\s+([-*]\s+\*\*)/g, "\n$1")
    .replace(/\s+(\d+[.)]\s+\*\*)/g, "\n$1")
    .replace(/\s+(>\s+)/g, "\n$1")
    .trim();
}

function renderInlineMarkdown(value: string) {
  const segments = value.split(/(`[^`]+`|\*\*[^*]+\*\*|__[^_]+__|\*[^*\n]+\*)/g).filter(Boolean);
  return segments.map((segment, index) => {
    if (segment.startsWith("`") && segment.endsWith("`")) {
      return <code key={`${segment}-${index}`}>{segment.slice(1, -1)}</code>;
    }
    if ((segment.startsWith("**") && segment.endsWith("**")) || (segment.startsWith("__") && segment.endsWith("__"))) {
      return <strong key={`${segment}-${index}`}>{segment.slice(2, -2)}</strong>;
    }
    if (segment.startsWith("*") && segment.endsWith("*")) {
      return <em key={`${segment}-${index}`}>{segment.slice(1, -1)}</em>;
    }
    return <span key={`${segment}-${index}`}>{segment}</span>;
  });
}

function renderMarkdownHeading(level: number, content: string) {
  if (level <= 1) {
    return <h3>{renderInlineMarkdown(content)}</h3>;
  }
  if (level === 2) {
    return <h4>{renderInlineMarkdown(content)}</h4>;
  }
  if (level === 3) {
    return <h5>{renderInlineMarkdown(content)}</h5>;
  }
  return <h6>{renderInlineMarkdown(content)}</h6>;
}

function MarkdownAnswer({ content }: { content: string }) {
  const normalized = normalizeMarkdownForPanel(content);
  const blocks = normalized.split(/\n{2,}/).filter(Boolean);
  return (
    <div className="prototype-markdown-answer">
      {blocks.map((block, blockIndex) => {
        const lines = block.split("\n").filter((line) => line.trim().length > 0);
        const firstLine = lines[0]?.trim() ?? "";
        const heading = firstLine.match(/^(#{1,6})\s+(.+)$/);
        if (heading) {
          const rest = lines.slice(1);
          return (
            <section key={`${blockIndex}-${firstLine}`}>
              {renderMarkdownHeading(heading[1].length, heading[2])}
              {rest.length > 0 ? <MarkdownAnswer content={rest.join("\n")} /> : null}
            </section>
          );
        }
        if (lines.every((line) => /^[-*]\s+/.test(line.trim()))) {
          return (
            <ul key={`${blockIndex}-${firstLine}`}>
              {lines.map((line, index) => (
                <li key={`${line}-${index}`}>{renderInlineMarkdown(line.trim().replace(/^[-*]\s+/, ""))}</li>
              ))}
            </ul>
          );
        }
        if (lines.every((line) => /^\d+[.)]\s+/.test(line.trim()))) {
          return (
            <ol key={`${blockIndex}-${firstLine}`}>
              {lines.map((line, index) => (
                <li key={`${line}-${index}`}>{renderInlineMarkdown(line.trim().replace(/^\d+[.)]\s+/, ""))}</li>
              ))}
            </ol>
          );
        }
        if (lines.every((line) => /^>\s+/.test(line.trim()))) {
          return (
            <blockquote key={`${blockIndex}-${firstLine}`}>
              {lines.map((line) => renderInlineMarkdown(line.trim().replace(/^>\s+/, "")))}
            </blockquote>
          );
        }
        return (
          <p key={`${blockIndex}-${firstLine}`}>
            {renderInlineMarkdown(lines.join(" "))}
          </p>
        );
      })}
    </div>
  );
}

function startTerminalThinkingIndicator(terminal: Terminal, label: string) {
  const frames = ["", ".", "..", "..."];
  let index = 0;
  const render = () => {
    terminal.write(`\r\x1b[2K\x1b[36m${label}${frames[index % frames.length]}\x1b[0m`);
    terminal.scrollToBottom();
    index += 1;
  };
  terminal.write("\r\n");
  render();
  const timer = window.setInterval(render, 600);
  return () => {
    window.clearInterval(timer);
    terminal.write("\r\x1b[2K");
  };
}

function normalizeShellCommand(command: string) {
  return command
    .trim()
    .replace(/\s+/g, " ")
    .replace(/;+\s*$/g, "");
}

function splitShellPipeline(command: string) {
  return command
    .split(/\s*(?:&&|\|\||\||;)\s*/g)
    .map((part) => part.trim())
    .filter(Boolean);
}

function firstCommandToken(segment: string) {
  const tokens = segment.trim().split(/\s+/);
  let index = 0;
  while (["sudo", "env", "command", "builtin", "time", "timeout"].includes(tokens[index] ?? "")) {
    index += tokens[index] === "timeout" ? 2 : 1;
  }
  return (tokens[index] ?? "").replace(/^['"]|['"]$/g, "");
}

function dangerousCommandPatternMatches(pattern: string, normalizedCommand: string) {
  const value = pattern.trim();
  if (!value) return false;
  try {
    return new RegExp(value, "i").test(normalizedCommand);
  } catch {
    return normalizedCommand.toLowerCase().includes(value.toLowerCase());
  }
}

function classifyAiCommand(command: string, purpose = "", dangerousCommands: string[] = []): ClassifiedCommand {
  const normalized = normalizeShellCommand(command);
  const lowered = normalized.toLowerCase();
  const dangerousPattern = dangerousCommands.find((item) => dangerousCommandPatternMatches(item, lowered));
  if (dangerousPattern) {
    return { command: normalized, purpose, risk: "blocked", reason: `命中危险命令黑名单：${dangerousPattern}` };
  }
  const absoluteBlockedPatterns: Array<[RegExp, string]> = [
    [/\brm\s+.*(-[^\s]*[rf][^\s]*|--recursive|--force)/i, "包含强制或递归删除命令"],
    [/\b(mkfs|fdisk|parted|dd)\b/i, "涉及磁盘分区、格式化或块级写入"],
    [/\b(shutdown|poweroff|halt)\b/i, "涉及主机关机"],
    [/\b(curl|wget)\b.*\|\s*(sh|bash|zsh)\b/i, "包含下载后直接执行脚本"],
    [/\b(mysql|psql|sqlite3)\b.*\b(drop|truncate)\b/i, "涉及数据库删除结构或清空数据"],
  ];
  for (const [pattern, reason] of absoluteBlockedPatterns) {
    if (pattern.test(lowered)) {
      return { command: normalized, purpose, risk: "blocked", reason };
    }
  }

  const highRiskPatterns: Array<[RegExp, string]> = [
    [/\brm\s+/i, "包含删除命令"],
    [/\b(reboot)\b/i, "涉及主机重启"],
    [/\b(systemctl|service)\s+(start|stop|restart|reload|enable|disable|mask|unmask)\b/i, "涉及服务状态变更"],
    [/\b(kill|pkill|killall)\b/i, "涉及终止进程"],
    [/\b(chmod|chown|chgrp|usermod|useradd|userdel|passwd)\b/i, "涉及权限或账号变更"],
    [/\b(apt|apt-get|yum|dnf|brew|pip|npm|pnpm|yarn)\s+(install|remove|upgrade|update|uninstall)\b/i, "涉及安装、卸载或升级软件"],
    [/\b(docker|podman)\s+(stop|start|restart|rm|rmi|kill|exec|run|compose)\b/i, "涉及容器变更或交互执行"],
    [/\b(kubectl)\s+(apply|delete|edit|exec|rollout|scale|patch|set)\b/i, "涉及 Kubernetes 变更"],
    [/\b(iptables|firewall-cmd)\s+.*(--add|--remove|--reload|-a|-d|-f)\b/i, "涉及防火墙变更"],
    [/\bfind\b.*\b-delete\b/i, "find 包含删除动作"],
    [/\bsed\s+-i\b/i, "包含原地修改文件"],
    [/(^|[^<])>{1,2}\s*\S+/i, "包含输出重定向写入"],
    [/\btee\s+/i, "包含写文件管道"],
    [/\b(mysql|psql|sqlite3)\b.*\b(delete|update|insert|alter|create)\b/i, "涉及数据库写入或结构变更"],
  ];
  for (const [pattern, reason] of highRiskPatterns) {
    if (pattern.test(lowered)) {
      return { command: normalized, purpose, risk: "high", reason };
    }
  }

  const reviewPatterns: Array<[RegExp, string]> = [
    [/\bsudo\s+/i, "包含 sudo，可能触发提权或交互式密码"],
    [/\bssh\s+/i, "包含二次 SSH 跳转"],
    [/\bscp|rsync\b/i, "涉及文件传输"],
    [/\bfind\b.*\b-exec\b/i, "find 包含执行动作"],
  ];
  let reviewReason = "";
  for (const [pattern, reason] of reviewPatterns) {
    if (pattern.test(lowered)) {
      reviewReason = reason;
      break;
    }
  }

  const safeCommands = new Set([
    "awk", "cat", "date", "df", "dmesg", "du", "free", "grep", "head", "hostname", "id", "ip",
    "firewall-cmd", "journalctl", "last", "lastb", "ls", "netstat", "ps", "pwd", "sed", "ss", "stat", "systemctl",
    "tail", "top", "uname", "uptime", "vmstat", "who", "whoami",
  ]);
  const safeSystemctl = /\bsystemctl\s+(status|show|list-units|list-timers|is-active|is-enabled)\b/i;
  const safeFirewallCmd = /\bfirewall-cmd\s+.*(--list|--get|--query|--state)/i;
  const safeDocker = /\bdocker\s+(ps|images|logs|inspect|stats|version|info)\b/i;
  const safeKubectl = /\bkubectl\s+(get|describe|logs|top|version)\b/i;
  const segments = splitShellPipeline(normalized);
  const allSegmentsReadonly = segments.length > 0 && segments.every((segment) => {
    const token = firstCommandToken(segment);
    if (token === "systemctl") {
      return safeSystemctl.test(segment);
    }
    if (token === "firewall-cmd") {
      return safeFirewallCmd.test(segment);
    }
    if (token === "docker") {
      return safeDocker.test(segment);
    }
    if (token === "kubectl") {
      return safeKubectl.test(segment);
    }
    if (token === "sed") {
      return !/\s-i(\s|$)/.test(segment);
    }
    if (token === "find") {
      return !/\b(exec|delete)\b/i.test(segment);
    }
    return safeCommands.has(token);
  });

  if (allSegmentsReadonly && !reviewReason) {
    return { command: normalized, purpose, risk: "readonly", reason: "只读查询命令" };
  }
  if (allSegmentsReadonly && reviewReason === "包含 sudo，可能触发提权或交互式密码") {
    return { command: normalized, purpose, risk: "readonly", reason: "sudo 只读查询命令，执行超时会自动返回错误" };
  }
  return { command: normalized, purpose, risk: "review", reason: reviewReason || "不在只读命令白名单内" };
}

function decideAiCommandByPolicy(policy: SshServerPolicy, command: ClassifiedCommand, aiUnrestricted = false): AiPolicyDecision {
  if (command.risk === "blocked") {
    return { action: "blocked", reason: `命令命中绝对禁止策略：${command.reason}` };
  }
  if (aiUnrestricted) {
    return { action: "auto", reason: "AI 临时放行已开启，30 分钟内允许自动执行读写命令；危险命令仍会阻止" };
  }
  if (policy === "blocked") {
    return { action: "blocked", reason: "当前服务器 AI 权限为禁用" };
  }
  if (command.risk === "readonly") {
    return { action: "auto", reason: "当前服务器 AI 权限允许自动执行只读命令" };
  }
  if (command.risk === "review") {
    if (policy === "L2" || policy === "L3") {
      return { action: "review", reason: `当前服务器 AI 权限为 ${sshPolicyLabel[policy]}，常规非只读命令需要用户审核` };
    }
    return { action: "blocked", reason: `当前服务器 AI 权限为 ${sshPolicyLabel[policy]}，不允许 AI 执行非只读命令` };
  }
  if (policy === "L3") {
    return { action: "review", reason: `当前服务器 AI 权限为 ${sshPolicyLabel[policy]}，高风险命令必须用户强确认` };
  }
  return { action: "blocked", reason: `当前服务器 AI 权限为 ${sshPolicyLabel[policy]}，不允许 AI 执行高风险命令` };
}

function detectRiskIntent(prompt: string): { risk: AiCommandRisk; reason: string } | null {
  const text = prompt.trim();
  const riskIntents: Array<[RegExp, AiCommandRisk, string]> = [
    [/(格式化|销毁|擦除|清空|关机|drop\s+table|truncate)/i, "blocked", "包含格式化、销毁、清空、关机或数据库 DROP/TRUNCATE 意图"],
    [/(删除|移除|重启|停止服务|启动服务|重载服务|杀掉|终止进程)/i, "high", "包含删除、重启、服务或进程状态变更意图"],
    [/(卸载|安装|升级|更新软件|修改权限|改权限|授权|改属主)/i, "high", "包含软件或权限变更意图"],
    [/(delete\s+from|update\s+\w+\s+set|insert\s+into)/i, "high", "包含数据库写入意图"],
  ];
  const matched = riskIntents.find(([pattern]) => pattern.test(text));
  return matched ? { risk: matched[1], reason: matched[2] } : null;
}

function commandPlanMarker(decision: AiPolicyDecision) {
  if (decision.action === "blocked") {
    return { text: "已禁止", color: "\x1b[31m" };
  }
  if (decision.action === "auto") {
    return { text: "自动执行", color: "\x1b[32m" };
  }
  return { text: "需审核", color: "\x1b[33m" };
}

function confirmReviewCommand(command: ClassifiedCommand, serverPolicy: SshServerPolicy, serverAlias: string, policyReason: string) {
  return new Promise<boolean>((resolve) => {
    let settled = false;
    const finish = (value: boolean) => {
      if (!settled) {
        settled = true;
        resolve(value);
      }
    };
    Modal.confirm({
      title: "命令需要审核确认",
      okText: "确认执行",
      cancelText: "取消",
      okButtonProps: { danger: true },
      maskClosable: false,
      closable: false,
      content: (
        <div className="prototype-approval-command">
          <p>服务器：{serverAlias}</p>
          <p>AI 权限：{sshPolicyLabel[serverPolicy] ?? serverPolicy}</p>
          <p>审核原因：{policyReason}</p>
          <p>风险原因：{command.reason}</p>
          <pre>{command.command}</pre>
          <p>确认后会立即在该服务器执行此命令。</p>
        </div>
      ),
      onOk: () => finish(true),
      onCancel: () => finish(false),
    });
  });
}

function commandPlanFromHeuristic(prompt: string): AiCommandPlanItem[] {
  const text = prompt.toLowerCase();
  if (text.includes("磁盘") || text.includes("内存") || text.includes("服务器情况") || text.includes("服务器状态") || text.includes("负载")) {
    return [
      { command: "uptime", purpose: "查看系统运行时长和负载", readonly: true },
      { command: "free -h", purpose: "查看内存使用情况", readonly: true },
      { command: "df -h", purpose: "查看磁盘空间使用情况", readonly: true },
    ];
  }
  if (text.includes("端口") || text.includes("监听")) {
    return [{ command: "ss -tuln", purpose: "查看 TCP/UDP 监听端口", readonly: true }];
  }
  if (text.includes("进程")) {
    return [{ command: "ps aux --sort=-%cpu | head -20", purpose: "查看 CPU 占用较高的进程", readonly: true }];
  }
  if (text.includes("服务")) {
    return [{ command: "systemctl list-units --type=service --state=running --no-pager", purpose: "查看正在运行的服务", readonly: true }];
  }
  return [];
}

function extractJsonArray(text: string): unknown[] {
  const fenced = text.match(/```(?:json)?\s*([\s\S]*?)```/i)?.[1];
  const source = fenced ?? text;
  const start = source.indexOf("[");
  const end = source.lastIndexOf("]");
  if (start < 0 || end <= start) {
    return [];
  }
  try {
    const parsed = JSON.parse(source.slice(start, end + 1)) as unknown;
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function parseAiCommandPlan(answer: string): AiCommandPlanItem[] {
  return extractJsonArray(answer).flatMap((item) => {
    if (!item || typeof item !== "object") {
      return [];
    }
    const value = item as Record<string, unknown>;
    const command = typeof value.command === "string" ? value.command.trim() : "";
    if (!command) {
      return [];
    }
    const risk: AiCommandRisk | undefined = value.risk === "readonly" || value.risk === "review" || value.risk === "high" || value.risk === "blocked"
      ? value.risk
      : undefined;
    return [{
      command,
      purpose: typeof value.purpose === "string" ? value.purpose.trim() : "AI 建议命令",
      risk,
      readonly: typeof value.readonly === "boolean" ? value.readonly : undefined,
    }];
  }).slice(0, 3);
}

const dangerousCommandPresets = [
  { pattern: String.raw`(?:^|[\s;&|])rm\s+-[a-z]*r[a-z]*f?[a-z]*\s+(?:/|~|\$home|\*)`, description: "rm -rf / ~" },
  { pattern: String.raw`(?:^|[\s;&|])rm\s+-[a-z]*f[a-z]*r[a-z]*\s+(?:/|~|\$home|\*)`, description: "rm -fr 等价形态" },
  { pattern: String.raw`\bmkfs[\.\w]*\b`, description: "mkfs.* 磁盘格式化" },
  { pattern: String.raw`\bmke2fs\b`, description: "mke2fs 磁盘格式化" },
  { pattern: String.raw`\bwipefs\b`, description: "wipefs 擦除文件系统签名" },
  { pattern: String.raw`\bdd\b[^\n]*\bof=/dev/`, description: "dd of=/dev/ 直接写块设备" },
  { pattern: String.raw`:\s*\(\s*\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:`, description: "fork bomb :(){:|:&};:" },
  { pattern: String.raw`>\s*/dev/sd[a-z]`, description: "重定向到 /dev/sd*" },
  { pattern: String.raw`\bchmod\s+-r\s+0*777\s+/(?:\s|$)`, description: "chmod -R 777 /" },
  { pattern: String.raw`\bchown\s+-r\s+\w+\s+/(?:\s|$)`, description: "chown -R x /" },
  { pattern: String.raw`\bshutdown\b`, description: "shutdown 关机" },
  { pattern: String.raw`\bpoweroff\b`, description: "poweroff 关机" },
  { pattern: String.raw`\bhalt\b`, description: "halt 停机" },
  { pattern: String.raw`\breboot\b`, description: "reboot 重启" },
  { pattern: String.raw`\binit\s+0\b`, description: "init 0 关机" },
  { pattern: String.raw`\biptables\s+-f\b`, description: "iptables -F 清空规则" },
  { pattern: String.raw`\bfirewall-cmd\b.*--reload\b`, description: "firewall-cmd reload" },
  { pattern: String.raw`\b(drop\s+database|drop\s+schema)\b`, description: "DROP DATABASE / SCHEMA" },
  { pattern: String.raw`\bdrop\s+table\b`, description: "DROP TABLE" },
  { pattern: String.raw`\btruncate\s+table\b`, description: "TRUNCATE TABLE" },
  { pattern: String.raw`\bflushall\b`, description: "Redis FLUSHALL" },
  { pattern: String.raw`\bflushdb\b`, description: "Redis FLUSHDB" },
  { pattern: String.raw`\b(curl|wget)\b.*\|\s*(sh|bash|zsh)\b`, description: "下载脚本后直接执行" },
  { pattern: String.raw`\b(find)\b.*\s-delete\b`, description: "find -delete 批量删除" },
] as const;

const dangerousPresetMap = new Map(dangerousCommandPresets.map((item) => [item.pattern, item.description]));

interface DangerousCommandTableRow {
  key: string;
  pattern: string;
  description: string;
  source: "builtin" | "user";
}

function DangerousCommandsField(_props: { value?: string[]; onChange?: (value: string[]) => void }) {
  return null;
}

function normalizeDangerousCommandList(commands: string[]) {
  const seen = new Set<string>();
  return commands
    .map((item) => item.trim())
    .filter(Boolean)
    .filter((item) => {
      const key = item.toLowerCase();
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
}

interface ProviderTemplate {
  key: string;
  name: string;
  region: AiProviderRegion;
  protocol: string;
  endpoint: string;
  authType: string;
  defaultModel: string;
  costLevel: AiProvider["costLevel"];
  capabilities: string[];
  models: string[];
  scenarioFit: string[];
  fallback: string;
}

const providerTemplates: ProviderTemplate[] = [
  {
    key: "anthropic",
    name: "Anthropic Claude",
    region: "global",
    protocol: "Messages API",
    endpoint: "https://api.anthropic.com",
    authType: "x-api-key",
    defaultModel: "claude-sonnet",
    costLevel: "高",
    capabilities: ["streaming", "tool_calling", "vision", "long_context"],
    models: ["Claude Sonnet", "Claude Opus", "Claude Haiku"],
    scenarioFit: ["长日志解释", "变更方案评审", "复杂上下文总结"],
    fallback: "openai",
  },
  {
    key: "openai",
    name: "OpenAI API",
    region: "global",
    protocol: "OpenAI Responses / Chat Completions",
    endpoint: "https://api.openai.com/v1",
    authType: "Bearer API Key",
    defaultModel: "gpt-4.1",
    costLevel: "高",
    capabilities: ["streaming", "tool_calling", "json_schema", "vision", "reasoning"],
    models: ["gpt-4.1", "gpt-4.1-mini", "gpt-5"],
    scenarioFit: ["高风险命令审查", "MCP 工具调用", "复杂排障"],
    fallback: "deepseek",
  },
  {
    key: "gemini",
    name: "Google Gemini",
    region: "global",
    protocol: "Gemini API",
    endpoint: "https://generativelanguage.googleapis.com",
    authType: "API Key",
    defaultModel: "gemini-pro",
    costLevel: "中",
    capabilities: ["streaming", "tool_calling", "vision", "long_context"],
    models: ["Gemini Pro", "Gemini Flash"],
    scenarioFit: ["长上下文分析", "多模态辅助", "低成本摘要"],
    fallback: "openai",
  },
  {
    key: "deepseek",
    name: "DeepSeek",
    region: "china",
    protocol: "OpenAI-compatible",
    endpoint: "https://api.deepseek.com",
    authType: "Bearer API Key",
    defaultModel: "deepseek-chat",
    costLevel: "低",
    capabilities: ["streaming", "tool_calling", "reasoning", "openai_compatible"],
    models: ["deepseek-chat", "deepseek-reasoner"],
    scenarioFit: ["命令生成", "代码解释", "低成本批量摘要"],
    fallback: "glm",
  },
  {
    key: "glm",
    name: "智谱 GLM",
    region: "china",
    protocol: "OpenAI-compatible / BigModel",
    endpoint: "https://open.bigmodel.cn/api/paas/v4",
    authType: "Bearer API Key",
    defaultModel: "glm-4-plus",
    costLevel: "中",
    capabilities: ["streaming", "tool_calling", "json_schema", "openai_compatible"],
    models: ["glm-4-plus", "glm-4-air", "glm-4-flash"],
    scenarioFit: ["中文问答", "MCP 工具调用", "结构化输出"],
    fallback: "kimi",
  },
  {
    key: "kimi",
    name: "Kimi / Moonshot",
    region: "china",
    protocol: "OpenAI-compatible",
    endpoint: "https://api.moonshot.cn/v1",
    authType: "Bearer API Key",
    defaultModel: "moonshot-v1-32k",
    costLevel: "中",
    capabilities: ["streaming", "long_context", "openai_compatible"],
    models: ["moonshot-v1-8k", "moonshot-v1-32k", "moonshot-v1-128k"],
    scenarioFit: ["长日志解释", "配置文件审阅", "审计摘要"],
    fallback: "deepseek",
  },
  {
    key: "minimax",
    name: "MiniMax",
    region: "china",
    protocol: "MiniMax API",
    endpoint: "https://api.minimax.chat",
    authType: "Bearer API Key",
    defaultModel: "abab6.5s",
    costLevel: "中",
    capabilities: ["streaming", "chat", "tool_calling"],
    models: ["abab6.5s", "abab6.5g"],
    scenarioFit: ["中文运维问答", "低延迟对话"],
    fallback: "glm",
  },
  {
    key: "xiaomi",
    name: "小米 MiMo",
    region: "china",
    protocol: "OpenAI-compatible / Anthropic-compatible",
    endpoint: "https://api.xiaomimimo.com/v1",
    authType: "api-key / Bearer API Key",
    defaultModel: "mimo-v1",
    costLevel: "中",
    capabilities: ["streaming", "openai_compatible", "anthropic_compatible"],
    models: ["mimo-v1"],
    scenarioFit: ["中文问答", "命令解释", "OpenAI 兼容接入"],
    fallback: "deepseek",
  },
];

function templateToFormValues(template: ProviderTemplate) {
  return {
    key: template.key,
    name: template.name,
    region: template.region,
    protocol: template.protocol,
    endpoint: template.endpoint,
    authType: template.authType,
    defaultModel: template.defaultModel,
    costLevel: template.costLevel,
    enabled: true,
  };
}

function getProviderSaveStatus(base: AiProvider | null, apiKeyValue: unknown): AiProvider["status"] {
  if (typeof apiKeyValue === "string" && apiKeyValue.trim()) {
    return "testing";
  }
  return base?.status ?? "unconfigured";
}

function uniqueNonEmpty(values: Array<string | null | undefined>) {
  return Array.from(new Set(values.map((value) => value?.trim()).filter((value): value is string => Boolean(value))));
}

function isConfiguredProvider(provider: AiProvider) {
  return provider.enabled && provider.status === "configured";
}

function formatDashboardTime(value: string | null | undefined) {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function approvalRiskTag(risk: string) {
  const riskColorMap: Record<string, string> = {
    readonly: "cyan",
    L1: "green",
    L2: "orange",
    L3: "red",
    review: "gold",
    high: "red",
    blocked: "red",
  };
  return <Tag color={riskColorMap[risk] ?? "default"}>{risk}</Tag>;
}

const serverColumns = [
  { title: "别名", dataIndex: "alias" },
  { title: "分组", dataIndex: "group" },
  { title: "地址", dataIndex: "host" },
  { title: "来源", dataIndex: "source" },
  { title: "认证", dataIndex: "auth" },
  { title: "AI 权限", dataIndex: "policy", render: (value: ServerRecord["policy"]) => <RiskBadge level={value} /> },
  { title: "状态", dataIndex: "status", render: (value: string) => <Tag color={value === "online" ? "green" : value === "web" ? "blue" : "orange"}>{value}</Tag> },
  { title: "操作", render: () => <Space><Button size="small">连接</Button><Button size="small">Tail</Button></Space> },
];

export function OnboardingPage() {
  return (
    <div className="prototype-page prototype-audit-page">
      <PageHeader title="启动引导" description="首次启动时完成资产导入、AI Provider、安全策略和 MCP Server 配置。" />
      <Card>
        <Steps
          current={1}
          items={[
            { title: "导入 SSH Config", content: "解析 Host、IdentityFile、ProxyJump" },
            { title: "配置 AI Provider", content: "Anthropic / OpenAI / Gemini / DeepSeek / GLM / Kimi / MiniMax / 小米" },
            { title: "设置权限策略", content: "只读、审批、拦截和审计" },
            { title: "启动 MCP Server", content: "生成客户端配置片段" },
          ]}
        />
      </Card>
      <SectionGrid columns={3}>
        <Card title="资产来源"><Paragraph>支持手工新增、`~/.ssh/config` 导入、JumpServer Web 会话引用。</Paragraph><Button type="primary">开始导入</Button></Card>
        <Card title="安全默认值"><Paragraph>写入、重启、删除默认进入审批。危险命令直接拦截并记录审计。</Paragraph><RiskBadge level="L2" label="默认审批" /></Card>
        <Card title="本机优先"><Paragraph>所有服务器、凭据和审计元数据默认存储在本机 SQLite。</Paragraph><Tag color="blue">local workspace</Tag></Card>
      </SectionGrid>
    </div>
  );
}

export function DashboardPage() {
  const navigate = useNavigate();
  const [loadingDashboard, setLoadingDashboard] = useState(false);
  const [dashboardServers, setDashboardServers] = useState<SshServer[]>([]);
  const [dashboardApprovals, setDashboardApprovals] = useState<ApprovalRequest[]>([]);
  const [dashboardAudits, setDashboardAudits] = useState<AuditLog[]>([]);
  const [dashboardProviders, setDashboardProviders] = useState<AiProvider[]>([]);
  const [dashboardJumpSessions, setDashboardJumpSessions] = useState<JumpServerSession[]>([]);
  const [dashboardMcp, setDashboardMcp] = useState<McpOverview | null>(null);

  const loadDashboard = useCallback(async () => {
    setLoadingDashboard(true);
    try {
      const [
        serverResult,
        approvalResult,
        auditResult,
        providerResult,
        jumpserverResult,
        mcpResult,
      ] = await Promise.allSettled([
        sshServerApi.list(),
        approvalApi.list({ status: "pending", limit: 20 }),
        auditApi.list({ limit: 10 }),
        aiProviderApi.list(),
        jumpserverApi.list(),
        mcpApi.overview(),
      ]);

      setDashboardServers(serverResult.status === "fulfilled" ? serverResult.value : []);
      setDashboardApprovals(approvalResult.status === "fulfilled" ? approvalResult.value : []);
      setDashboardAudits(auditResult.status === "fulfilled" ? auditResult.value : []);
      setDashboardProviders(providerResult.status === "fulfilled" ? providerResult.value : []);
      setDashboardJumpSessions(jumpserverResult.status === "fulfilled" ? jumpserverResult.value : []);
      setDashboardMcp(mcpResult.status === "fulfilled" ? mcpResult.value : null);

      const firstRejected = [
        serverResult,
        approvalResult,
        auditResult,
        providerResult,
        jumpserverResult,
        mcpResult,
      ].find((item) => item.status === "rejected");
      if (firstRejected?.status === "rejected") {
        message.warning(`部分工作台数据加载失败：${getErrorMessage(firstRejected.reason)}`);
      }
    } finally {
      setLoadingDashboard(false);
    }
  }, []);

  useEffect(() => {
    void loadDashboard();
  }, [loadDashboard]);

  const enabledServers = dashboardServers.filter((item) => item.enabled);
  const onlineServers = dashboardServers.filter((item) => item.status === "online");
  const configuredProviders = dashboardProviders.filter(isConfiguredProvider);
  const activeJumpSessions = dashboardJumpSessions.filter((item) => item.enabled && item.status !== "disabled");
  const recentServers = [...dashboardServers]
    .sort((left, right) => {
      const leftTime = new Date(left.lastConnectedAt ?? left.updatedAt).getTime();
      const rightTime = new Date(right.lastConnectedAt ?? right.updatedAt).getTime();
      return rightTime - leftTime;
    })
    .slice(0, 6);

  const dashboardStats = [
    {
      label: "服务器资产",
      value: String(dashboardServers.length),
      hint: `${enabledServers.length} 台启用，${onlineServers.length} 台在线`,
    },
    {
      label: "待审批请求",
      value: String(dashboardApprovals.length),
      hint: dashboardApprovals.length > 0 ? "存在需人工确认的 AI 操作" : "暂无待处理审批",
    },
    {
      label: "近期审计",
      value: String(dashboardAudits.length),
      hint: "展示最近 10 条操作记录",
    },
    {
      label: "AI Provider",
      value: String(configuredProviders.length),
      hint: `共 ${dashboardProviders.length} 个 Provider，MCP 工具 ${dashboardMcp?.tools.length ?? 0} 个`,
    },
  ];

  const dashboardServerColumns: TableProps<SshServer>["columns"] = [
    { title: "别名", dataIndex: "alias" },
    { title: "分组", dataIndex: "groupName" },
    {
      title: "地址",
      render: (_, record) => `${record.username}@${record.host}:${record.port}`,
    },
    {
      title: "AI 权限",
      dataIndex: "aiPolicy",
      render: (value: SshServerPolicy) => <Tag color={sshPolicyColor[value]}>{sshPolicyLabel[value]}</Tag>,
    },
    {
      title: "状态",
      dataIndex: "status",
      render: (value: SshServer["status"]) => {
        const meta = sshServerStatusMeta[value] ?? sshServerStatusMeta.unknown;
        return <Badge status={meta.status} text={meta.text} />;
      },
    },
    {
      title: "最近连接",
      dataIndex: "lastConnectedAt",
      render: (value: string | null) => formatDashboardTime(value),
    },
    {
      title: "操作",
      width: 110,
      render: (_, record) => (
        <Button
          size="small"
          type="primary"
          disabled={!record.enabled}
          onClick={() => navigate(`/terminal?server=${encodeURIComponent(record.alias)}&connect=1&source=dashboard&request=${Date.now()}`)}
        >
          连接
        </Button>
      ),
    },
  ];

  const dashboardApprovalColumns: TableProps<ApprovalRequest>["columns"] = [
    { title: "编号", dataIndex: "id", width: 80 },
    { title: "来源", dataIndex: "source", width: 110 },
    { title: "服务器", dataIndex: "serverAlias", width: 150 },
    { title: "动作", dataIndex: "action", width: 140 },
    { title: "风险", dataIndex: "risk", width: 90, render: (value: string) => approvalRiskTag(value) },
    { title: "摘要", dataIndex: "summary", ellipsis: true },
    { title: "创建时间", dataIndex: "createdAt", width: 130, render: (value: string) => formatDashboardTime(value) },
  ];

  const dashboardAuditColumns: TableProps<AuditLog>["columns"] = [
    { title: "时间", dataIndex: "occurredAt", width: 130, render: (value: string) => formatDashboardTime(value) },
    {
      title: "来源",
      dataIndex: "source",
      width: 170,
      render: (value: string) => <Text style={{ whiteSpace: "nowrap" }}>{value}</Text>,
    },
    { title: "服务器", dataIndex: "serverAlias", width: 150, render: (value: string) => value || "-" },
    { title: "动作", dataIndex: "action", width: 207 },
    { title: "结果", dataIndex: "result", width: 80, render: (value: string) => <Tag color={value === "success" ? "green" : value === "blocked" ? "red" : "orange"}>{value}</Tag> },
    { title: "摘要", dataIndex: "summary", width: 360, ellipsis: true },
  ];

  return (
    <div className="prototype-page">
      <PageHeader
        title="工作台"
        description="集中展示真实服务器资产、待审批请求、近期审计、AI Provider 与 MCP Server 状态。"
        actions={(
          <Space>
            <Button onClick={() => void loadDashboard()} loading={loadingDashboard} icon={<RefreshCw size={14} />}>刷新</Button>
            <Button type="primary" onClick={() => navigate("/servers")}>新建 SSH 会话</Button>
          </Space>
        )}
      />
      <SectionGrid columns={4}>
        {dashboardStats.map((item) => <StatCard key={item.label} {...item} />)}
      </SectionGrid>
      <Card
        title="服务器快捷入口"
        extra={<Button size="small" onClick={() => navigate("/servers")}>管理服务器</Button>}
      >
        <Table
          size="small"
          loading={loadingDashboard}
          pagination={false}
          rowKey="alias"
          columns={dashboardServerColumns}
          dataSource={recentServers}
        />
      </Card>
      <Card
        title="待审批"
        extra={<Button size="small" onClick={() => navigate("/approval")}>进入审批队列</Button>}
      >
        <Table
          size="small"
          loading={loadingDashboard}
          pagination={false}
          rowKey="id"
          columns={dashboardApprovalColumns}
          dataSource={dashboardApprovals}
        />
      </Card>
      <Card
        title="近期审计"
        extra={<Button size="small" onClick={() => navigate("/audit")}>查看审计日志</Button>}
      >
        <Table
          size="small"
          loading={loadingDashboard}
          pagination={false}
          rowKey="id"
          columns={dashboardAuditColumns}
          dataSource={dashboardAudits}
        />
      </Card>
      <AiInsightPanel title="运行状态">
        <Space direction="vertical" size={12} style={{ width: "100%" }}>
          <div className="flex items-center justify-between gap-3">
            <Text type="secondary">MCP Server</Text>
            {dashboardMcp ? (
              <Badge status={dashboardMcp.status.httpReachable ? "success" : "warning"} text={dashboardMcp.status.httpReachable ? "HTTP 可用" : "本机配置可用"} />
            ) : (
              <Tag>未加载</Tag>
            )}
          </div>
          <div className="flex items-center justify-between gap-3">
            <Text type="secondary">Agent 客户端</Text>
            <Text>{dashboardMcp?.clients.filter((item) => item.configured).length ?? 0}/{dashboardMcp?.clients.length ?? 0} 已配置</Text>
          </div>
          <div className="flex items-center justify-between gap-3">
            <Text type="secondary">堡垒机会话</Text>
            <Text>{activeJumpSessions.length} 个可用引用</Text>
          </div>
          <Divider style={{ margin: "4px 0" }} />
          <Paragraph style={{ marginBottom: 0 }}>
            工作台数据来自本机 SQLite 与 Tauri 后端 Command。可从这里快速进入服务器连接、审批处理和审计追踪。
          </Paragraph>
          <CodeBlock style={{ marginBottom: 0 }}>{dashboardMcp?.status.streamableHttpUrl ?? "MCP Server 地址将在后端启动后显示"}</CodeBlock>
        </Space>
      </AiInsightPanel>
    </div>
  );
}

export function ServersPage() {
  const navigate = useNavigate();
  const [serverForm] = Form.useForm();
  const [serverList, setServerList] = useState<SshServer[]>([]);
  const [serverCredentialList, setServerCredentialList] = useState<CredentialVaultItem[]>([]);
  const [selectedServer, setSelectedServer] = useState<SshServer | null>(null);
  const [serverDrawerOpen, setServerDrawerOpen] = useState(false);
  const [loadingServers, setLoadingServers] = useState(false);
  const [testingServerForm, setTestingServerForm] = useState(false);
  const [importingSshConfig, setImportingSshConfig] = useState(false);
  const selectedAuthType = Form.useWatch("authType", serverForm) as UpsertSshServerInput["authType"] | undefined;
  const effectiveAuthType = selectedAuthType ?? selectedServer?.authType ?? "key";

  async function loadSshServers() {
    setLoadingServers(true);
    try {
      const items = await sshServerApi.list();
      setServerList(items);
    } catch (error) {
      message.error(getErrorMessage(error));
      setServerList([]);
    } finally {
      setLoadingServers(false);
    }
  }

  useEffect(() => {
    void loadSshServers();
    void loadServerCredentials();
  }, []);

  async function loadServerCredentials() {
    try {
      setServerCredentialList(await credentialVaultApi.list());
    } catch {
      setServerCredentialList([]);
    }
  }

  const passwordCredentialOptions = useMemo(() => serverCredentialList
    .filter((item) => item.credentialType === "password" && item.enabled)
    .map((item) => ({
      value: `vault:${item.key}`,
      label: `${item.key}${item.scope ? `（${item.scope}）` : ""}`,
    })), [serverCredentialList]);

  useEffect(() => {
    if (!serverDrawerOpen) {
      return;
    }
    serverForm.resetFields();
    const formValues = selectedServer
      ? {
          ...selectedServer,
          identityFile: selectedServer.identityFile || selectedServer.authRef.replace(/^key:/, ""),
        }
      : {
      enabled: true,
      source: "manual",
      authType: "direct_password",
      aiPolicy: "L2",
      port: 22,
      status: "unknown",
    };
    serverForm.setFieldsValue(formValues);
  }, [serverDrawerOpen, selectedServer, serverForm]);

  async function handleImportSshConfig() {
    setImportingSshConfig(true);
    try {
      const result = await sshServerApi.importSshConfig();
      setServerList(result.servers);
      message.success(`已导入 ${result.imported} 个 SSH Host，跳过 ${result.skipped} 个通配 Host`);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setImportingSshConfig(false);
    }
  }

  async function handleSaveServer(values: Record<string, unknown>) {
    const alias = String(values.alias ?? selectedServer?.alias ?? "").trim();
    if (!alias) {
      message.warning("请填写服务器别名");
      return;
    }
    const input: UpsertSshServerInput = {
      alias,
      groupName: String(values.groupName ?? "默认").trim() || "默认",
      host: String(values.host ?? "").trim(),
      port: Number(values.port ?? 22),
      username: String(values.username ?? "").trim(),
      source: (values.source ?? "manual") as SshServerSource,
      authType: (values.authType ?? "key") as UpsertSshServerInput["authType"],
      authRef: values.authType === "key"
        ? (String(values.identityFile ?? "").trim() ? `key:${String(values.identityFile).trim()}` : "")
        : values.authType === "direct_password"
          ? String(values.authRef ?? selectedServer?.authRef ?? `password:${alias}`).trim()
          : String(values.authRef ?? "").trim(),
      identityFile: values.authType === "key" ? String(values.identityFile ?? "").trim() : "",
      password: values.authType === "direct_password" && values.password ? String(values.password) : null,
      clearPassword: false,
      proxyJump: String(values.proxyJump ?? "").trim(),
      aiPolicy: (values.aiPolicy ?? "L2") as SshServerPolicy,
      status: selectedServer?.status ?? "unknown",
      enabled: Boolean(values.enabled ?? true),
    };
    try {
      await sshServerApi.upsert(input);
      message.success("服务器配置已保存");
      setServerDrawerOpen(false);
      setSelectedServer(null);
      await loadSshServers();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function handleDeleteServer(alias: string) {
    try {
      await sshServerApi.delete(alias);
      message.success("服务器已删除");
      await loadSshServers();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function handleTestServerForm() {
    setTestingServerForm(true);
    try {
      const values = await serverForm.validateFields(["alias", "host", "port"]);
      const alias = String(values.alias ?? selectedServer?.alias ?? "").trim();
      const host = String(values.host ?? "").trim();
      const port = Number(values.port ?? 22);
      const result = await sshServerApi.testConnection({
        alias: alias || null,
        host,
        port,
      });
      if (result.ok) {
        message.success(`${result.endpoint} 连接成功：${result.latencyMs} ms`);
      } else {
        message.warning(`${result.endpoint} 连接未通过：${result.message}`);
      }
    } catch (error) {
      if (typeof error === "object" && error !== null && "errorFields" in error) {
        message.warning("请先填写服务器别名、主机地址和端口");
        return;
      }
      message.error(getErrorMessage(error));
    } finally {
      setTestingServerForm(false);
    }
  }

  return (
    <div className="prototype-page">
      <PageHeader
        title="服务器管理"
        description="按分组管理 SSH 服务器，展示来源、认证引用、AI 权限和连接状态。"
        actions={
          <Space>
            <Button loading={importingSshConfig} onClick={() => void handleImportSshConfig()}>导入 SSH Config</Button>
            <Button type="primary" onClick={() => { setSelectedServer(null); setServerDrawerOpen(true); }}>新增服务器</Button>
          </Space>
        }
      />
      <Card>
        <Table<SshServer>
          rowKey="alias"
          loading={loadingServers}
          dataSource={serverList}
          pagination={{ pageSize: 10, size: "small" }}
          locale={{ emptyText: "暂无服务器，可导入 SSH Config 或手工新增" }}
          columns={[
            {
              title: "别名",
              dataIndex: "alias",
              render: (value: string, record) => (
                <Space orientation="vertical" size={0}>
                  <Text strong>{value}</Text>
                  {record.username ? <Text type="secondary">{record.username}</Text> : null}
                </Space>
              ),
            },
            { title: "分组", dataIndex: "groupName" },
            { title: "地址", render: (_, record) => formatServerAddress(record) },
            { title: "来源", dataIndex: "source", render: (value: SshServerSource) => formatSshServerSource(value) },
            { title: "认证", dataIndex: "authRef", render: (_, record) => formatSshServerAuth(record) },
            { title: "AI 权限", dataIndex: "aiPolicy", render: (value: SshServerPolicy) => <Tag>{sshPolicyLabel[value] ?? value}</Tag> },
            {
              title: "状态",
              dataIndex: "status",
              render: (value: SshServer["status"]) => {
                const meta = sshServerStatusMeta[value] ?? sshServerStatusMeta.unknown;
                return <Tag color={meta.color}>{meta.text}</Tag>;
              },
            },
            {
              title: "操作",
              width: 170,
              render: (_, record) => (
                <Space size={8}>
                  <Button
                    size="small"
                    onClick={() => navigate(`/terminal?server=${encodeURIComponent(record.alias)}&connect=1&source=servers&request=${Date.now()}`)}
                  >
                    连接
                  </Button>
                  <Button size="small" onClick={() => { setSelectedServer(record); setServerDrawerOpen(true); }}>编辑</Button>
                  <Popconfirm title="删除服务器？" okText="删除" cancelText="取消" onConfirm={() => void handleDeleteServer(record.alias)}>
                    <Button size="small" danger>删除</Button>
                  </Popconfirm>
                </Space>
              ),
            },
          ]}
        />
      </Card>
      <Drawer
        title={selectedServer ? `编辑 ${selectedServer.alias}` : "新增服务器"}
        open={serverDrawerOpen}
        onClose={() => { setServerDrawerOpen(false); setSelectedServer(null); }}
        size="large"
      >
        <Form form={serverForm} layout="vertical" onFinish={(values) => void handleSaveServer(values)}>
          <SectionGrid columns={3}>
            <Form.Item label="服务器别名" name="alias" rules={[{ required: true, message: "请填写服务器别名" }]}>
              <Input disabled={Boolean(selectedServer)} placeholder="prod-api-01" />
            </Form.Item>
            <Form.Item label="分组" name="groupName" rules={[{ required: true, message: "请填写分组" }]}>
              <Input placeholder="生产 / API" />
            </Form.Item>
            <Form.Item label="主机地址" name="host" rules={[{ required: true, message: "请填写主机地址" }]}>
              <Input placeholder="10.18.2.21" />
            </Form.Item>
            <Form.Item label="端口" name="port" rules={[{ required: true, message: "请填写端口" }]}>
              <Input type="number" min={1} max={65535} />
            </Form.Item>
            <Form.Item label="用户名" name="username">
              <Input placeholder="root / deploy" />
            </Form.Item>
            <Form.Item label="来源" name="source">
              <Select options={[{ value: "manual", label: "手工维护" }, { value: "ssh_config", label: "SSH Config" }, { value: "jumpserver", label: "JumpServer" }]} />
            </Form.Item>
            <Form.Item label="认证方式" name="authType">
              <Select
                options={[
                  { value: "direct_password", label: "直接密码" },
                  { value: "key", label: "密钥文件" },
                  { value: "password_ref", label: "密码引用" },
                  { value: "session_reference", label: "会话引用" },
                ]}
                onChange={(value) => {
                  if (value === "key") {
                    serverForm.setFieldValue("authRef", "");
                    serverForm.setFieldValue("password", "");
                  } else if (value === "direct_password") {
                    serverForm.setFieldValue("identityFile", "");
                    serverForm.setFieldValue("authRef", selectedServer?.authRef ?? "");
                  } else {
                    serverForm.setFieldValue("identityFile", "");
                    serverForm.setFieldValue("password", "");
                  }
                }}
              />
            </Form.Item>
            {effectiveAuthType === "key" ? (
              <Form.Item label="私钥文件（IdentityFile）" name="identityFile">
                <Input placeholder="~/.ssh/id_rsa" />
              </Form.Item>
            ) : effectiveAuthType === "direct_password" ? (
              <Form.Item label="登录密码" name="password" rules={[{ required: !selectedServer?.hasPassword, message: "请填写登录密码" }]}>
                <Input.Password placeholder={selectedServer?.hasPassword ? "留空则保留已保存密码" : "请输入 SSH 登录密码"} />
              </Form.Item>
            ) : (
              <Form.Item label="认证引用" name="authRef" rules={[{ required: true, message: "请填写认证引用" }]}>
                {effectiveAuthType === "password_ref" ? (
                  <Select
                    showSearch
                    optionFilterProp="label"
                    placeholder="选择凭据保险库中的密码凭据"
                    options={passwordCredentialOptions}
                    notFoundContent="暂无密码凭据，请先到凭据保险库新增"
                  />
                ) : (
                  <Input placeholder="session:jumpserver-prod" />
                )}
              </Form.Item>
            )}
            <Form.Item label="ProxyJump / 跳板" name="proxyJump">
              <Select
                allowClear
                showSearch
                placeholder="选择跳板服务器"
                optionFilterProp="label"
                options={serverList
                  .filter((item) => item.alias !== selectedServer?.alias)
                  .map((item) => ({
                    value: item.alias,
                    label: `${item.alias} (${item.groupName})`,
                  }))}
              />
            </Form.Item>
            <Form.Item label="AI 权限" name="aiPolicy">
              <Select options={sshPolicyOptions} />
            </Form.Item>
            <Form.Item label="启用" name="enabled" valuePropName="checked">
              <Switch />
            </Form.Item>
          </SectionGrid>
          <Alert title="直接密码只会发送给 Rust 后端加密存储；编辑时不回显明文，留空表示保留已保存密码。测试连接按钮只执行 TCP 连通性测试。" type="info" showIcon />
          <Divider />
          <div className="flex w-full items-center justify-between">
            <Space size={16}>
              <Button style={{ width: 100, height: 30 }} onClick={() => { setServerDrawerOpen(false); setSelectedServer(null); }}>取消</Button>
              <Button style={{ width: 100, height: 30 }} type="primary" htmlType="submit">保存服务器</Button>
            </Space>
            <Button
              htmlType="button"
              loading={testingServerForm}
              style={{ width: 100, height: 30 }}
              onClick={() => void handleTestServerForm()}
            >
              测试连接
            </Button>
          </div>
        </Form>
      </Drawer>
    </div>
  );
}

export function ServerFormPage() {
  return (
    <div className="prototype-page">
      <PageHeader title="服务器表单" description="新增或编辑服务器配置，凭据只保存加密引用，预留团队字段。" />
      <Card>
        <Form layout="vertical" initialValues={{ source: "manual", authType: "key", aiPolicy: "L2", workspace: "local-personal" }}>
          <SectionGrid columns={3}>
            <Form.Item label="服务器别名" name="alias"><Input placeholder="prod-api-01" /></Form.Item>
            <Form.Item label="主机地址" name="host"><Input placeholder="10.18.2.21" /></Form.Item>
            <Form.Item label="端口" name="port"><Input placeholder="22" /></Form.Item>
            <Form.Item label="分组" name="group"><Select options={[{ value: "生产 / API" }, { value: "预发" }, { value: "堡垒机" }]} /></Form.Item>
            <Form.Item label="来源" name="source"><Select options={[{ value: "manual" }, { value: "ssh_config" }, { value: "jumpserver" }]} /></Form.Item>
            <Form.Item label="认证方式" name="authType"><Select options={[{ value: "key" }, { value: "password" }, { value: "session_reference" }]} /></Form.Item>
            <Form.Item label="ProxyJump / 跳板" name="proxy"><Input placeholder="bastion-prod" /></Form.Item>
            <Form.Item label="AI 权限" name="aiPolicy"><Select options={[{ value: "readonly" }, { value: "L1" }, { value: "L2" }, { value: "blocked" }]} /></Form.Item>
            <Form.Item label="工作区预留" name="workspace"><Input /></Form.Item>
          </SectionGrid>
          <Alert title="凭据只保存加密 secret payload。前端、AI、MCP 客户端和审计日志均不可见明文。" type="info" showIcon />
        </Form>
      </Card>
    </div>
  );
}

export function SshImportPage() {
  const rows = servers.filter((item) => item.source !== "manual");
  return (
    <div className="prototype-page">
      <PageHeader title="SSH Config 导入" description="解析本机 SSH Config，处理冲突、分组映射和凭据引用。" actions={<Button type="primary">选择配置文件</Button>} />
      <SectionGrid columns={3}>
        <Card title="解析结果"><Progress percent={100} /><Paragraph>发现 12 个 Host，8 个可直接导入，2 个需要处理 ProxyJump，2 个重复。</Paragraph></Card>
        <Card title="冲突策略"><Checkbox defaultChecked>保留已有别名</Checkbox><br /><Checkbox defaultChecked>重复 Host 加后缀</Checkbox><br /><Checkbox>覆盖旧配置</Checkbox></Card>
        <Card title="分组映射"><Tag>prod-* → 生产</Tag><Tag>stage-* → 预发</Tag><Tag>jump-* → 堡垒机</Tag></Card>
      </SectionGrid>
      <Card title="导入预览"><Table size="small" pagination={false} rowKey="alias" columns={serverColumns.slice(0, 7)} dataSource={rows} /></Card>
    </div>
  );
}

export function VaultPage() {
  const [credentialForm] = Form.useForm();
  const [scopeForm] = Form.useForm();
  const [rotateForm] = Form.useForm();
  const [credentials, setCredentials] = useState<CredentialVaultItem[]>([]);
  const [vaultServerList, setVaultServerList] = useState<SshServer[]>([]);
  const [loadingCredentials, setLoadingCredentials] = useState(false);
  const [credentialDrawerOpen, setCredentialDrawerOpen] = useState(false);
  const [selectedCredential, setSelectedCredential] = useState<CredentialVaultItem | null>(null);
  const [editingCredential, setEditingCredential] = useState<CredentialVaultItem | null>(null);
  const [scopeModalOpen, setScopeModalOpen] = useState(false);
  const [rotateModalOpen, setRotateModalOpen] = useState(false);
  const [savingCredential, setSavingCredential] = useState(false);

  async function loadCredentials() {
    setLoadingCredentials(true);
    try {
      setCredentials(await credentialVaultApi.list());
    } catch (error) {
      message.error(getErrorMessage(error));
      setCredentials([]);
    } finally {
      setLoadingCredentials(false);
    }
  }

  async function loadCredentialScopeOptions() {
    try {
      setVaultServerList(await sshServerApi.list());
    } catch {
      setVaultServerList([]);
    }
  }

  useEffect(() => {
    void loadCredentials();
    void loadCredentialScopeOptions();
  }, []);

  const credentialScopeOptions = useMemo(() => {
    const groups = Array.from(new Set(vaultServerList.map((server) => server.groupName).filter(Boolean))).map((group) => ({
      label: `分组：${group}`,
      value: group,
    }));
    const servers = vaultServerList.map((server) => ({
      label: `服务器：${server.alias}`,
      value: server.alias,
    }));
    return [
      { label: "全部服务器", value: "all" },
      ...groups,
      ...servers,
    ];
  }, [vaultServerList]);

  function openCredentialDrawer() {
    setSelectedCredential(null);
    setEditingCredential(null);
    credentialForm.resetFields();
    credentialForm.setFieldsValue({
      credentialType: "password",
      status: "normal",
      enabled: true,
    });
    setCredentialDrawerOpen(true);
  }

  function openEditCredentialDrawer(item: CredentialVaultItem) {
    setSelectedCredential(null);
    setEditingCredential(item);
    credentialForm.resetFields();
    credentialForm.setFieldsValue({
      key: item.key,
      credentialType: item.credentialType,
      scope: item.scope,
      status: item.status,
      description: item.description,
      secret: "",
      enabled: item.enabled,
    });
    setCredentialDrawerOpen(true);
  }

  async function handleSaveCredential(values: Record<string, unknown>) {
    const key = String(values.key ?? editingCredential?.key ?? "").trim();
    const secret = String(values.secret ?? "").trim();
    const input: UpsertCredentialInput = {
      key,
      credentialType: (values.credentialType ?? "password") as CredentialType,
      scope: String(values.scope ?? "").trim(),
      status: (values.status ?? "normal") as CredentialStatus,
      description: String(values.description ?? "").trim(),
      secret: secret ? secret : null,
      clearSecret: false,
      enabled: Boolean(values.enabled ?? true),
    };
    setSavingCredential(true);
    try {
      await credentialVaultApi.upsert(input);
      message.success("凭据已保存");
      setCredentialDrawerOpen(false);
      setEditingCredential(null);
      await loadCredentials();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSavingCredential(false);
    }
  }

  function openScopeModal(item: CredentialVaultItem) {
    setSelectedCredential(item);
    scopeForm.setFieldsValue({ scope: item.scope });
    setScopeModalOpen(true);
  }

  async function handleAuthorizeCredential() {
    if (!selectedCredential) {
      return;
    }
    try {
      const values = await scopeForm.validateFields();
      await credentialVaultApi.authorize({
        key: selectedCredential.key,
        scope: String(values.scope ?? "").trim(),
      });
      message.success("授权范围已更新");
      setScopeModalOpen(false);
      setSelectedCredential(null);
      await loadCredentials();
    } catch (error) {
      if (typeof error === "object" && error !== null && "errorFields" in error) {
        return;
      }
      message.error(getErrorMessage(error));
    }
  }

  function openRotateModal(item: CredentialVaultItem) {
    setSelectedCredential(item);
    rotateForm.resetFields();
    setRotateModalOpen(true);
  }

  async function handleRotateCredential() {
    if (!selectedCredential) {
      return;
    }
    try {
      const values = await rotateForm.validateFields();
      await credentialVaultApi.rotate({
        key: selectedCredential.key,
        secret: String(values.secret ?? ""),
      });
      message.success("凭据已轮换");
      setRotateModalOpen(false);
      setSelectedCredential(null);
      await loadCredentials();
    } catch (error) {
      if (typeof error === "object" && error !== null && "errorFields" in error) {
        return;
      }
      message.error(getErrorMessage(error));
    }
  }

  async function handleDeleteCredential(key: string) {
    try {
      await credentialVaultApi.delete(key);
      message.success("凭据已删除");
      await loadCredentials();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  return (
    <div className="prototype-page">
      <PageHeader title="凭据保险库" description="本机加密保存凭据引用，支持轮换、授权范围和非明文会话引用。" actions={<Button type="primary" onClick={openCredentialDrawer}>新增凭据</Button>} />
      <Card>
        <Table<CredentialVaultItem>
          loading={loadingCredentials}
          pagination={{ pageSize: 10, size: "small" }}
          rowKey="key"
          dataSource={credentials}
          locale={{ emptyText: "暂无凭据，点击右上角新增凭据" }}
          columns={[
            { title: "凭据", dataIndex: "key", render: (value: string, record) => (
              <Space orientation="vertical" size={0}>
                <Text strong>{value}</Text>
                {record.description ? <Text type="secondary">{record.description}</Text> : null}
              </Space>
            ) },
            { title: "类型", dataIndex: "credentialType", render: (value: CredentialType) => credentialTypeLabel[value] ?? value },
            { title: "授权范围", dataIndex: "scope" },
            { title: "状态", dataIndex: "status", render: (value: CredentialStatus) => {
              const meta = credentialStatusMeta[value] ?? credentialStatusMeta.normal;
              return <Tag color={meta.color}>{meta.text}</Tag>;
            } },
            { title: "密文", dataIndex: "hasSecret", render: (value: boolean, record) => value ? record.secretMasked : <Text type="secondary">未保存</Text> },
            { title: "轮换", dataIndex: "rotatedAt", render: (value: string | null) => formatCredentialRotatedAt(value) },
            {
              title: "操作",
              width: 220,
              render: (_, record) => (
                <Space size={8}>
                  <Button size="small" onClick={() => openScopeModal(record)}>授权</Button>
                  <Button size="small" onClick={() => openEditCredentialDrawer(record)}>编辑</Button>
                  <Button size="small" onClick={() => openRotateModal(record)}>轮换</Button>
                  <Popconfirm title="删除凭据？" okText="删除" cancelText="取消" onConfirm={() => void handleDeleteCredential(record.key)}>
                    <Button size="small" danger>删除</Button>
                  </Popconfirm>
                </Space>
              ),
            },
          ]}
        />
      </Card>
      <Drawer
        title={editingCredential ? `编辑凭据：${editingCredential.key}` : "新增凭据"}
        open={credentialDrawerOpen}
        onClose={() => {
          setCredentialDrawerOpen(false);
          setEditingCredential(null);
        }}
        width={560}
      >
        <Form form={credentialForm} layout="vertical" onFinish={(values) => void handleSaveCredential(values)}>
          <Form.Item label="凭据 Key" name="key" rules={[{ required: true, message: "请填写凭据 Key" }]}>
            <Input disabled={Boolean(editingCredential)} placeholder="password-prod-api" />
          </Form.Item>
          <Form.Item label="类型" name="credentialType" rules={[{ required: true, message: "请选择类型" }]}>
            <Select options={credentialTypeOptions} />
          </Form.Item>
          <Form.Item label="授权范围" name="scope" rules={[{ required: true, message: "请选择授权范围" }]}>
            <Select
              showSearch
              optionFilterProp="label"
              placeholder="选择服务器分组或具体服务器"
              options={credentialScopeOptions}
            />
          </Form.Item>
          <Form.Item label="状态" name="status">
            <Select options={Object.entries(credentialStatusMeta).map(([value, meta]) => ({ value, label: meta.text }))} />
          </Form.Item>
          <Form.Item
            label="凭据内容"
            name="secret"
            rules={editingCredential?.hasSecret ? [] : [{ required: true, message: "请填写凭据内容" }]}
          >
            <Input.TextArea
              rows={5}
              placeholder={editingCredential?.hasSecret ? "留空则保留已保存密文；填写则更新密文" : "密码、私钥、Token 或会话引用内容"}
            />
          </Form.Item>
          <Form.Item label="说明" name="description">
            <Input placeholder="用途说明，非敏感信息" />
          </Form.Item>
          <Form.Item label="启用" name="enabled" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Alert title="凭据内容只发送给 Rust 后端加密保存；列表、AI、MCP 与审计日志均不返回明文。" type="info" showIcon />
          <Divider />
          <Space>
            <Button onClick={() => setCredentialDrawerOpen(false)}>取消</Button>
            <Button type="primary" htmlType="submit" loading={savingCredential}>保存凭据</Button>
          </Space>
        </Form>
      </Drawer>
      <Drawer
        title={selectedCredential ? `授权 ${selectedCredential.key}` : "授权范围"}
        open={scopeModalOpen}
        onClose={() => setScopeModalOpen(false)}
        width={480}
      >
        <Form form={scopeForm} layout="vertical">
          <Form.Item label="授权范围" name="scope" rules={[{ required: true, message: "请选择授权范围" }]}>
            <Select
              showSearch
              optionFilterProp="label"
              placeholder="选择服务器分组或具体服务器"
              options={credentialScopeOptions}
            />
          </Form.Item>
          <Alert title="授权范围用于限制该凭据可绑定或可被引用的服务器、分组或会话。" type="info" showIcon />
          <Divider />
          <Space>
            <Button onClick={() => setScopeModalOpen(false)}>取消</Button>
            <Button type="primary" onClick={() => void handleAuthorizeCredential()}>保存授权</Button>
          </Space>
        </Form>
      </Drawer>
      <Drawer
        title={selectedCredential ? `轮换 ${selectedCredential.key}` : "轮换凭据"}
        open={rotateModalOpen}
        onClose={() => setRotateModalOpen(false)}
        width={560}
      >
        <Form form={rotateForm} layout="vertical">
          <Form.Item label="新凭据内容" name="secret" rules={[{ required: true, message: "请填写新凭据内容" }]}>
            <Input.TextArea rows={5} placeholder="新的密码、私钥、Token 或会话引用内容" />
          </Form.Item>
          <Alert title="轮换会覆盖后端保存的密文，并将状态重置为正常，前端仍不会回显明文。" type="warning" showIcon />
          <Divider />
          <Space>
            <Button onClick={() => setRotateModalOpen(false)}>取消</Button>
            <Button type="primary" onClick={() => void handleRotateCredential()}>确认轮换</Button>
          </Space>
        </Form>
      </Drawer>
    </div>
  );
}

export function TerminalPage() {
  const location = useLocation();
  const [terminalServers, setTerminalServers] = useState<SshServer[]>([]);
  const [selectedAlias, setSelectedAlias] = useState<string>();
  const [terminalTabs, setTerminalTabs] = useState<TerminalTabState[]>(() => terminalWorkspace.tabs);
  const [activeTerminalId, setActiveTerminalId] = useState<string | undefined>(() => (
    terminalWorkspace.activeId ?? terminalWorkspace.tabs[terminalWorkspace.tabs.length - 1]?.id
  ));
  const [terminalMaximized, setTerminalMaximized] = useState(false);
  const [aiQuestion, setAiQuestion] = useState("");
  const [aiAnswer, setAiAnswer] = useState("打开一个 SSH 终端后，可以让 AI 解释当前标签输出、判断风险或生成下一步排障建议。");
  const [terminalAiAsking, setTerminalAiAsking] = useState(false);
  const [terminalExperienceSaving, setTerminalExperienceSaving] = useState(false);
  const terminalTabsRef = useRef<TerminalTabState[]>(terminalWorkspace.tabs);
  const activeTerminalIdRef = useRef<string | undefined>(terminalWorkspace.activeId);
  const terminalHostsRef = useRef(terminalWorkspace.hosts);
  const terminalContextsRef = useRef(terminalWorkspace.contexts);
  const terminalSeqRef = useRef(terminalWorkspace.seq);
  const autoOpenRequestRef = useRef<string | null>(terminalWorkspace.handledRequestKey);
  const unlistenRef = useRef<UnlistenFn | null>(null);
  const activeTab = terminalTabs.find((item) => item.id === activeTerminalId);
  const activeServerPolicy = activeTab
    ? terminalServers.find((server) => server.alias === activeTab.serverAlias)?.aiPolicy
    : undefined;
  const connectedCount = terminalTabs.filter((item) => item.connected).length;
  const selectedAliasOpening = terminalTabs.some((item) => item.serverAlias === selectedAlias && item.connecting);

  function updateTerminalTab(tabId: string, patch: Partial<{
    title: string;
    status: string;
    connected: boolean;
    connecting: boolean;
    risk: TerminalRiskLevel;
    transcript: string[];
    aiMessages: TerminalAiMessage[];
  }>) {
    setTerminalTabs((prev) => {
      const next = prev.map((tab) => (tab.id === tabId ? { ...tab, ...patch } : tab));
      terminalTabsRef.current = next;
      terminalWorkspace.tabs = next;
      return next;
    });
  }

  function appendTranscript(tabId: string, value: string) {
    const clean = value
      .replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, "")
      .replace(/\r/g, "\n")
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
    if (clean.length > 0) {
      setTerminalTabs((prev) => {
        const next = prev.map((tab) => (
          tab.id === tabId
            ? { ...tab, transcript: [...tab.transcript.slice(-180), ...clean].slice(-200) }
            : tab
        ));
        terminalTabsRef.current = next;
        terminalWorkspace.tabs = next;
        return next;
      });
    }
  }

  function terminalAiDefaultAnswer() {
    return "打开一个 SSH 终端后，可以让 AI 解释当前标签输出、判断风险或生成下一步排障建议。";
  }

  function normalizeTerminalAiMessages(tab?: TerminalTabState) {
    return (tab?.aiMessages ?? []).slice(-12);
  }

  function formatTerminalAiHistory(messages: TerminalAiMessage[]) {
    if (messages.length === 0) {
      return "暂无历史对话。";
    }
    return messages
      .map((item) => `${item.role === "user" ? "用户" : "AI"}：${truncateText(item.content, 1_200)}`)
      .join("\n\n");
  }

  function appendTerminalAiExchange(tabId: string, question: string, answer: string) {
    const now = new Date().toISOString();
    setTerminalTabs((prev) => {
      const next = prev.map((tab) => {
        if (tab.id !== tabId) {
          return tab;
        }
        return {
          ...tab,
          aiMessages: [
            ...normalizeTerminalAiMessages(tab),
            { role: "user" as const, content: question, createdAt: now },
            { role: "assistant" as const, content: answer, createdAt: now },
          ].slice(-12),
        };
      });
      terminalTabsRef.current = next;
      terminalWorkspace.tabs = next;
      return next;
    });
  }

  function buildTerminalAiConversationPrompt(tab: TerminalTabState, question: string) {
    const recentTranscript = tab.transcript.slice(-80).join("\n");
    return [
      `服务器标签：${tab.serverAlias}`,
      `当前终端状态：${tab.status}`,
      `最近终端输出：\n${recentTranscript ? truncateText(recentTranscript, 8_000) : "暂无可用终端输出。"}`,
      `本标签历史 AI 对话：\n${formatTerminalAiHistory(normalizeTerminalAiMessages(tab))}`,
      `用户本轮问题：${question}`,
    ].join("\n\n");
  }

  function findTabIdBySession(sessionId: string) {
    for (const [tabId, context] of terminalContextsRef.current.entries()) {
      if (context.sessionId === sessionId) {
        return tabId;
      }
    }
    return undefined;
  }

  const handleTerminalEvent = useCallback((payload: TerminalSessionEvent) => {
    const tabId = payload.sessionId ? findTabIdBySession(payload.sessionId) : undefined;
    if (!tabId) {
      return;
    }
    const context = terminalContextsRef.current.get(tabId);
    const terminal = context?.terminal;
    if (payload.kind === "connecting" && payload.message) {
      updateTerminalTab(tabId, { status: payload.message, connected: false, connecting: true });
      terminal?.writeln(`\r\n\x1b[36m${payload.message}\x1b[0m`);
      return;
    }
    if (payload.kind === "data" && payload.data) {
      terminal?.write(payload.data);
      appendTranscript(tabId, payload.data);
      return;
    }
    if (payload.kind === "status" && payload.message) {
      updateTerminalTab(tabId, { status: payload.message, connected: true, connecting: false });
      if (context) {
        context.connected = true;
      }
      terminal?.writeln(`\r\n\x1b[32m${payload.message}\x1b[0m`);
      setAiAnswer("SSH 终端已连接。可以直接输入命令；Ctrl+C、方向键、Tab 补全等控制序列会进入远端 PTY。");
      fitTerminal(tabId);
      return;
    }
    if (payload.kind === "error") {
      const messageText = payload.message ?? "SSH 终端会话发生错误";
      updateTerminalTab(tabId, { status: messageText, risk: "review", connected: false, connecting: false });
      if (context) {
        context.connected = false;
      }
      terminal?.writeln(`\r\n\x1b[31m${messageText}\x1b[0m`);
      message.error(messageText);
      return;
    }
    if (payload.kind === "exit") {
      const messageText = payload.message ?? "SSH 终端会话已结束";
      updateTerminalTab(tabId, { status: messageText, connected: false, connecting: false });
      if (context) {
        context.connected = false;
      }
      terminal?.writeln(`\r\n\x1b[33m${messageText}\x1b[0m`);
    }
  }, []);

  function fitTerminal(tabId = activeTerminalId) {
    if (!tabId) {
      return;
    }
    try {
      const context = terminalContextsRef.current.get(tabId);
      if (!context) {
        return;
      }
      context.fitAddon.fit();
      if (context.terminal.rows > TERMINAL_BOTTOM_RESERVED_ROWS + 4) {
        context.terminal.resize(
          context.terminal.cols,
          context.terminal.rows - TERMINAL_BOTTOM_RESERVED_ROWS,
        );
      }
      if (!context.connected) {
        return;
      }
      if (hasTauriRuntime() && context.sessionId) {
        void terminalApi.resizeSession({
          sessionId: context.sessionId,
          cols: context.terminal.cols,
          rows: context.terminal.rows,
        });
      } else if (context.websocket?.readyState === WebSocket.OPEN) {
        context.websocket.send(JSON.stringify({
          type: "resize",
          cols: context.terminal.cols,
          rows: context.terminal.rows,
        }));
      }
    } catch {
      // xterm 容器首次渲染前可能没有尺寸，忽略本次适配。
    }
  }

  function writeToRemoteTerminal(context: TerminalContext, data: string) {
    if (hasTauriRuntime() && context.sessionId) {
      void terminalApi.writeSession({ sessionId: context.sessionId, data });
    } else if (context.websocket?.readyState === WebSocket.OPEN) {
      context.websocket.send(JSON.stringify({ type: "data", data }));
    }
  }

  function restoreTerminalInputReady(context: TerminalContext) {
    context.inputBuffer = "";
    context.inputCursorIndex = 0;
    context.aiLineMode = false;
    context.aiBusy = false;
    context.terminal.write("\r\n");
    if (context.connected) {
      // AI 输出是本地写入 xterm 的，不经过远端 PTY；补一个回车让远端 shell 重新打印真实提示符。
      writeToRemoteTerminal(context, "\r");
    }
    setTimeout(() => {
      context.terminal.scrollToBottom();
      context.terminal.focus();
    }, 0);
  }

  function renderAiInputLine(context: TerminalContext) {
    const chars = Array.from(context.inputBuffer);
    const tail = chars.slice(context.inputCursorIndex).join("");
    const tailWidth = textCellWidth(tail);
    context.terminal.write(`\r\x1b[2K\x1b[36mAI> \x1b[0m${context.inputBuffer}`);
    if (tailWidth > 0) {
      context.terminal.write(`\x1b[${tailWidth}D`);
    }
  }

  function insertAiInputText(context: TerminalContext, value: string) {
    const chars = Array.from(context.inputBuffer);
    const insertChars = Array.from(value);
    chars.splice(context.inputCursorIndex, 0, ...insertChars);
    context.inputBuffer = chars.join("");
    context.inputCursorIndex += insertChars.length;
    renderAiInputLine(context);
  }

  function handleTerminalInput(tabId: string, data: string) {
    const context = terminalContextsRef.current.get(tabId);
    if (!context?.connected) {
      context?.terminal.write("\r\n请等待 SSH 连接建立。\r\n");
      return;
    }

    const isEnter = data === "\r" || data === "\n" || data === "\r\n";
    const isBackspace = data === "\u007f" || data === "\b";
    const isDelete = data === "\x1b[3~";
    const isArrowLeft = data === "\x1b[D" || data === "\x1bOD";
    const isArrowRight = data === "\x1b[C" || data === "\x1bOC";
    const isHome = data === "\x1b[H" || data === "\x1b[1~" || data === "\x1bOH";
    const isEnd = data === "\x1b[F" || data === "\x1b[4~" || data === "\x1bOF";
    const isCancel = data === "\u0003" || data === "\x1b";
    const isControlSequence = data.startsWith("\x1b") || (data.length === 1 && data.charCodeAt(0) < 32 && !isEnter);

    if (!context.aiLineMode && context.inputBuffer.length === 0 && startsWithChinese(data)) {
      context.aiLineMode = true;
      context.inputBuffer = "";
      context.inputCursorIndex = 0;
      context.terminal.write("\r\n");
    }

    if (!context.aiLineMode) {
      if (isEnter) {
        context.inputBuffer = "";
        context.inputCursorIndex = 0;
      } else if (isBackspace) {
        const chars = Array.from(context.inputBuffer);
        chars.pop();
        context.inputBuffer = chars.join("");
        context.inputCursorIndex = chars.length;
      } else if (!isControlSequence) {
        context.inputBuffer += data;
        context.inputCursorIndex = Array.from(context.inputBuffer).length;
      }
      writeToRemoteTerminal(context, data);
      return;
    }

    if (context.aiBusy) {
      context.terminal.write("\r\nAI 正在回答，请稍候。\r\n");
      return;
    }

    if (isCancel) {
      context.inputBuffer = "";
      context.inputCursorIndex = 0;
      context.aiLineMode = false;
      context.terminal.write("\r\x1b[2K\x1b[33m已取消 AI 输入。\x1b[0m\r\n");
      return;
    }

    if (isEnter) {
      const prompt = context.inputBuffer.trim();
      context.inputBuffer = "";
      context.inputCursorIndex = 0;
      context.aiLineMode = false;
      context.terminal.write("\r\n");
      if (prompt) {
        void askAiFromTerminal(tabId, prompt);
      }
      return;
    }

    if (isBackspace) {
      if (context.inputCursorIndex > 0) {
        const chars = Array.from(context.inputBuffer);
        chars.splice(context.inputCursorIndex - 1, 1);
        context.inputBuffer = chars.join("");
        context.inputCursorIndex -= 1;
        renderAiInputLine(context);
      }
      return;
    }

    if (isDelete) {
      const chars = Array.from(context.inputBuffer);
      if (context.inputCursorIndex < chars.length) {
        chars.splice(context.inputCursorIndex, 1);
        context.inputBuffer = chars.join("");
        renderAiInputLine(context);
      }
      return;
    }

    if (isArrowLeft) {
      if (context.inputCursorIndex > 0) {
        context.inputCursorIndex -= 1;
        renderAiInputLine(context);
      }
      return;
    }

    if (isArrowRight) {
      if (context.inputCursorIndex < Array.from(context.inputBuffer).length) {
        context.inputCursorIndex += 1;
        renderAiInputLine(context);
      }
      return;
    }

    if (isHome) {
      context.inputCursorIndex = 0;
      renderAiInputLine(context);
      return;
    }

    if (isEnd) {
      context.inputCursorIndex = Array.from(context.inputBuffer).length;
      renderAiInputLine(context);
      return;
    }

    if (isControlSequence) {
      return;
    }

    insertAiInputText(context, data);
  }

  function createTerminalAiAudit(
    serverAlias: string,
    action: string,
    risk: AuditRisk,
    result: string,
    summary: string,
    detail: Record<string, unknown>,
  ) {
    void auditApi.create({
      actor: "local-user",
      source: "terminal-ai",
      serverAlias,
      action,
      risk,
      result,
      summary,
      detailJson: JSON.stringify(detail),
      requestId: null,
      approvalId: null,
    }).catch(() => undefined);
  }

  async function askAiFromTerminal(tabId: string, prompt: string) {
    const context = terminalContextsRef.current.get(tabId);
    const tab = terminalTabsRef.current.find((item) => item.id === tabId);
    if (!context || !tab) {
      return;
    }
    const server = terminalServers.find((item) => item.alias === tab.serverAlias);
    const serverPolicy = server?.aiPolicy ?? "blocked";
    const [aiUnrestrictedState, currentSettings] = await Promise.all([
      systemSettingsApi.getAiUnrestrictedState().catch(() => ({ active: false, until: null, remainingSeconds: 0 })),
      systemSettingsApi.get().catch(() => null),
    ]);
    const aiUnrestricted = aiUnrestrictedState.active;
    const dangerousCommands = currentSettings?.dangerousCommands ?? [];
    const auditIfEnabled = (
      action: string,
      risk: AuditRisk,
      result: string,
      summary: string,
      detail: Record<string, unknown>,
    ) => {
      if (aiUnrestricted) {
        return;
      }
      createTerminalAiAudit(tab.serverAlias, action, risk, result, summary, detail);
    };
    context.aiBusy = true;
    let stopThinking: (() => void) | null = null;
    const stopCurrentThinking = () => {
      stopThinking?.();
      stopThinking = null;
    };
    updateTerminalTab(tabId, {
      status: "正在调用 AI 生成命令计划...",
      risk: "safe",
      transcript: [...tab.transcript.slice(-180), `AI> ${prompt}`].slice(-200),
    });
    auditIfEnabled("terminal_ai_prompt", "ai", "成功", `终端 AI 请求：${truncateText(prompt, 120)}`, {
      serverAlias: tab.serverAlias,
      aiPolicy: serverPolicy,
      aiUnrestricted,
      prompt: truncateText(prompt, 500),
    });
    try {
      stopThinking = startTerminalThinkingIndicator(context.terminal, "AI 思考中，正在生成只读检查计划");
      const planResult = await aiProviderApi.ask({
        prompt: [
          "请基于下面的连续终端会话上下文，为用户本轮问题规划最多 3 条 SSH 命令。",
          buildTerminalAiConversationPrompt(tab, prompt),
        ].join("\n\n"),
        skillScope: "terminal",
        useSkillTrigger: true,
        systemPrompt: [
          "你是 Tauri SSH 的终端 AI 命令规划器。",
          `当前服务器标签是 ${tab.serverAlias}，服务器 AI 权限策略是 ${sshPolicyLabel[serverPolicy] ?? serverPolicy}。`,
          aiUnrestricted ? "当前已开启 30 分钟 AI 临时放行：非危险命令可自动执行，危险命令黑名单仍必须标注 blocked。" : "",
          "优先规划 Linux 查询/诊断类命令，最多 3 条。若用户明确要求变更类操作，可以返回命令但必须标注风险。",
          "必须只返回 JSON 数组，不要 Markdown，不要解释文本。",
          "数组项字段：command 字符串、purpose 中文字符串、risk 只能是 readonly/review/high/blocked、readonly 布尔值。",
          "risk 判定：查询诊断为 readonly；文件传输、二次 SSH、非白名单命令为 review；服务变更、重启、安装卸载、权限变更、写文件、数据库写入为 high；强制递归删除、格式化、关机、下载脚本直接执行、数据库 DROP/TRUNCATE 为 blocked。",
        ].join("\n"),
      });
      stopCurrentThinking();

      const parsedPlan = parseAiCommandPlan(planResult.answer);
      const heuristicPlan = parsedPlan.length > 0 ? [] : commandPlanFromHeuristic(prompt);
      const plan = (parsedPlan.length > 0 ? parsedPlan : heuristicPlan)
        .map((item) => classifyAiCommand(item.command, item.purpose, dangerousCommands))
        .slice(0, 3);
      const policyPlan = plan.map((item) => ({
        item,
        decision: decideAiCommandByPolicy(serverPolicy, item, aiUnrestricted),
      }));
      const autoPlan = policyPlan.filter(({ decision }) => decision.action === "auto").map(({ item }) => item);
      const reviewPlan = policyPlan.filter(({ decision }) => decision.action === "review");
      const blockedPlan = policyPlan.filter(({ decision }) => decision.action === "blocked");

      writeWrappedTerminalLine(context.terminal, `AI 计划 (${planResult.providerName} / ${planResult.model}, ${planResult.latencyMs}ms)`, { color: "\x1b[36m" });
      writeWrappedTerminalLine(context.terminal, `服务器 AI 权限：${sshPolicyLabel[serverPolicy] ?? serverPolicy}`);
      if (aiUnrestricted) {
        writeWrappedTerminalLine(context.terminal, `AI 临时放行：已开启，剩余约 ${Math.ceil(aiUnrestrictedState.remainingSeconds / 60)} 分钟；本次跳过 AI 审计`, { color: "\x1b[33m" });
      }
      if (plan.length === 0) {
        const intent = detectRiskIntent(prompt);
        const intentDecision = intent
          ? decideAiCommandByPolicy(serverPolicy, {
            command: "未生成明确命令",
            purpose: prompt,
            risk: intent.risk,
            reason: intent.reason,
          }, aiUnrestricted)
          : null;
        const answer = intent
          ? `${intentDecision?.action === "review" ? "需要审核" : "已禁止执行"}：当前请求未生成明确命令，且${intent.reason}。请先生成或输入明确命令后再按当前服务器 AI 权限级别处理。${intentDecision ? `策略判断：${intentDecision.reason}` : ""}`
          : "AI 没有生成可自动执行的只读命令。请换一种更明确的查询描述，或在右侧 AI 面板中提问获取人工确认方案。";
        const risk = intentDecision?.action === "blocked" ? "blocked" : "review";
        writeWrappedTerminalBlock(context.terminal, answer, risk === "blocked" ? "\x1b[31m" : "\x1b[33m");
        updateTerminalTab(tabId, { status: intent ? answer : "AI 未生成可执行计划", risk });
        setAiAnswer(answer);
        appendTerminalAiExchange(tabId, prompt, answer);
        auditIfEnabled("terminal_ai_no_plan", risk === "blocked" ? "blocked" : "L2", "未执行", answer, {
          prompt: truncateText(prompt, 500),
          reason: intent?.reason ?? "AI 未生成可执行命令",
        });
        return;
      }

      policyPlan.forEach(({ item, decision }, index) => {
        const marker = commandPlanMarker(decision);
        const color = marker.color;
        writeWrappedTerminalLine(context.terminal, `[${marker.text}] ${item.command}`, {
          color,
          firstPrefix: `${index + 1}. `,
          nextPrefix: "   ",
        });
        writeWrappedTerminalLine(context.terminal, item.purpose, { firstPrefix: "   目的：", nextPrefix: "         " });
        writeWrappedTerminalLine(context.terminal, `命令风险：${item.risk}；风险原因：${item.reason}`, {
          firstPrefix: "   风险：",
          nextPrefix: "         ",
        });
        writeWrappedTerminalLine(context.terminal, decision.reason, {
          firstPrefix: "   策略：",
          nextPrefix: "         ",
        });
      });

      if (serverPolicy === "blocked" && !aiUnrestricted) {
        const answer = [
          "当前服务器 AI 权限为禁用，禁止 AI 执行任何命令。",
          ...plan.map((item) => `- 已禁止：${item.command}；原因：服务器 AI 权限为禁用`),
        ].join("\n");
        writeWrappedTerminalBlock(context.terminal, answer, "\x1b[31m");
        updateTerminalTab(tabId, { status: answer, risk: "blocked" });
        setAiAnswer(answer);
        appendTerminalAiExchange(tabId, prompt, answer);
        auditIfEnabled("terminal_ai_blocked_by_policy", "blocked", "已禁止", answer, {
          aiPolicy: serverPolicy,
          commandCount: plan.length,
          commands: plan.map((item) => truncateText(item.command, 300)),
        });
        return;
      }

      if (blockedPlan.length > 0) {
        blockedPlan.forEach(({ item, decision }) => {
          writeWrappedTerminalLine(context.terminal, item.command, { color: "\x1b[31m", firstPrefix: "已禁止执行：", nextPrefix: "              " });
          writeWrappedTerminalLine(context.terminal, decision.reason, { color: "\x1b[31m", firstPrefix: "禁止原因：", nextPrefix: "          " });
        });
      }

      const approvedReviewPlan: ClassifiedCommand[] = [];
      for (const { item, decision } of reviewPlan) {
        writeWrappedTerminalLine(context.terminal, item.command, { color: "\x1b[33m", firstPrefix: "等待用户审核确认：", nextPrefix: "                  " });
        writeWrappedTerminalLine(context.terminal, decision.reason, { color: "\x1b[33m", firstPrefix: "审核原因：", nextPrefix: "          " });
        const approved = await confirmReviewCommand(item, serverPolicy, tab.serverAlias, decision.reason);
        if (approved) {
          approvedReviewPlan.push(item);
          writeWrappedTerminalLine(context.terminal, item.command, { color: "\x1b[32m", firstPrefix: "审核通过：", nextPrefix: "          " });
        } else {
          writeWrappedTerminalLine(context.terminal, item.command, { color: "\x1b[33m", firstPrefix: "用户取消审核命令：", nextPrefix: "                  " });
        }
      }

      const executablePlan = [...autoPlan, ...approvedReviewPlan];
      const cancelledReviewPlan = reviewPlan.filter(({ item }) => !approvedReviewPlan.includes(item));
      const rejectedPlan = [...blockedPlan, ...cancelledReviewPlan];

      if (executablePlan.length === 0) {
        const answer = [
          blockedPlan.length > 0 ? "未执行：存在禁止命令，已按安全策略阻止。" : "",
          cancelledReviewPlan.length > 0 ? "未执行：需要审核的命令已被用户取消。" : "",
          ...blockedPlan.map(({ item, decision }) => `- 已禁止：${item.command}；${decision.reason}`),
          ...cancelledReviewPlan.map(({ item, decision }) => `- 已取消：${item.command}；${decision.reason}`),
        ].filter(Boolean).join("\n");
        writeWrappedTerminalBlock(context.terminal, answer, blockedPlan.length > 0 ? "\x1b[31m" : "\x1b[33m");
        updateTerminalTab(tabId, { status: answer || "未执行任何命令", risk: blockedPlan.length > 0 ? "blocked" : "review" });
        setAiAnswer(answer || "未执行任何命令。");
        appendTerminalAiExchange(tabId, prompt, answer || "未执行任何命令。");
        auditIfEnabled("terminal_ai_not_executed", blockedPlan.length > 0 ? "blocked" : "L2", "未执行", answer || "未执行任何命令。", {
          blockedCommands: blockedPlan.map(({ item, decision }) => ({
            command: truncateText(item.command, 300),
            reason: decision.reason,
          })),
          cancelledCommands: cancelledReviewPlan.map(({ item, decision }) => ({
            command: truncateText(item.command, 300),
            reason: decision.reason,
          })),
        });
        return;
      }

      updateTerminalTab(tabId, {
        status: `正在执行 ${executablePlan.length} 条命令...`,
        risk: rejectedPlan.length > 0 || approvedReviewPlan.length > 0 ? "review" : "safe",
      });

      const executions: AiCommandExecution[] = [];
      for (const item of executablePlan) {
        context.terminal.writeln("");
        writeWrappedTerminalLine(context.terminal, item.command, { color: "\x1b[32m", firstPrefix: "$ ", nextPrefix: "  ", preserveSpaces: true });
        try {
          const result = await terminalApi.execute({
            serverAlias: tab.serverAlias,
            command: item.command,
            timeoutSecs: 30,
            initiatedByAi: true,
          });
          executions.push({ plan: item, result });
          if (result.stdout) {
            truncateText(result.stdout, 6_000).split("\n").forEach((line) => writeWrappedTerminalLine(context.terminal, line, { preserveSpaces: true }));
          }
          if (result.stderr) {
            truncateText(result.stderr, 3_000).split("\n").forEach((line) => writeWrappedTerminalLine(context.terminal, line, { color: "\x1b[33m", preserveSpaces: true }));
          }
          context.terminal.writeln(`\x1b[90m退出码 ${result.exitStatus}，耗时 ${result.durationMs}ms\x1b[0m`);
        } catch (error) {
          const errorMessage = getErrorMessage(error);
          context.terminal.writeln(`\x1b[31m执行失败：${errorMessage}\x1b[0m`);
        }
      }

      if (executions.length === 0) {
        const answer = "命令计划通过了策略，但实际执行失败，未获得可汇总的输出。";
        updateTerminalTab(tabId, { status: answer, risk: "review" });
        setAiAnswer(answer);
        appendTerminalAiExchange(tabId, prompt, answer);
        return;
      }

      const executionText = executions.map(({ plan: item, result }) => [
        `命令：${item.command}`,
        `目的：${item.purpose}`,
        `退出码：${result.exitStatus}`,
        `stdout:\n${truncateText(result.stdout || "(空)", 4_000)}`,
        `stderr:\n${truncateText(result.stderr || "(空)", 1_200)}`,
      ].join("\n")).join("\n\n---\n\n");
      updateTerminalTab(tabId, {
        status: "AI 正在汇总执行结果...",
        risk: rejectedPlan.length > 0 || approvedReviewPlan.length > 0 ? "review" : "safe",
      });
      stopThinking = startTerminalThinkingIndicator(context.terminal, "AI 思考中，正在汇总执行结果");
      const summaryResult = await aiProviderApi.ask({
        prompt: [
          `本标签历史 AI 对话：\n${formatTerminalAiHistory(normalizeTerminalAiMessages(tab))}`,
          `用户请求：${prompt}`,
          `服务器：${tab.serverAlias}`,
          `AI 权限策略：${sshPolicyLabel[serverPolicy] ?? serverPolicy}`,
          `自动执行结果：\n${truncateText(executionText, 12_000)}`,
          rejectedPlan.length > 0 ? `未执行命令：\n${rejectedPlan.map(({ item, decision }) => `${item.command}：${decision.reason}`).join("\n")}` : "",
        ].filter(Boolean).join("\n\n"),
        skillScope: "terminal",
        useSkillTrigger: true,
        systemPrompt: "你是 SSH 运维结果分析助手。请基于真实命令输出，用中文给出简洁汇总、异常点、下一步建议。不要声称执行未执行的命令。",
      });
      stopCurrentThinking();
      const header = `AI 汇总 (${summaryResult.providerName} / ${summaryResult.model}, ${summaryResult.latencyMs}ms)`;
      context.terminal.writeln(`\r\n\x1b[36m${header}\x1b[0m`);
      writeMarkdownToTerminal(context.terminal, summaryResult.answer);
      appendTerminalAiExchange(tabId, prompt, summaryResult.answer);
      const transcriptLines = [
        `AI> ${prompt}`,
        ...executions.map(({ plan: item, result }) => `$ ${item.command}\n${result.stdout}${result.stderr ? `\n${result.stderr}` : ""}`),
        summaryResult.answer,
      ];
      updateTerminalTab(tabId, {
        status: `已执行 ${executions.length} 条命令并汇总`,
        risk: rejectedPlan.length > 0 || approvedReviewPlan.length > 0 ? "review" : "safe",
        transcript: [...tab.transcript.slice(-120), ...transcriptLines].slice(-200),
      });
      setAiAnswer(summaryResult.answer);
    } catch (error) {
      stopCurrentThinking();
      const errorMessage = getErrorMessage(error);
      context.terminal.writeln(`\x1b[31mAI 调用失败：${errorMessage}\x1b[0m`);
      updateTerminalTab(tabId, { status: `AI 调用失败：${errorMessage}`, risk: "review" });
      message.error(errorMessage);
    } finally {
      stopCurrentThinking();
      restoreTerminalInputReady(context);
    }
  }

  function ensureTerminal(tabId: string) {
    const existingContext = terminalContextsRef.current.get(tabId);
    if (existingContext) {
      const host = terminalHostsRef.current.get(tabId);
      const terminalElement = existingContext.terminal.element;
      if (host && terminalElement && terminalElement.parentElement !== host) {
        host.replaceChildren(terminalElement);
        existingContext.resizeObserver.disconnect();
        existingContext.resizeObserver = new ResizeObserver(() => fitTerminal(tabId));
        existingContext.resizeObserver.observe(host);
        setTimeout(() => fitTerminal(tabId), 0);
      }
      return existingContext;
    }
    const host = terminalHostsRef.current.get(tabId);
    if (!host) {
      return undefined;
    }
    const terminal = new Terminal({
      cursorBlink: true,
      fontFamily: "Menlo, Monaco, Consolas, monospace",
      fontSize: 13,
      lineHeight: 1.2,
      scrollback: 5000,
      theme: {
        background: "#101413",
        foreground: "#d7efe8",
        cursor: "#79d6b4",
        selectionBackground: "#23423a",
      },
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(host);
    fitAddon.fit();
    terminal.writeln("\x1b[32mTauri SSH\x1b[0m");
    terminal.writeln("正在准备 SSH 终端...");
    const dataDisposable = terminal.onData((data) => {
      handleTerminalInput(tabId, data);
    });
    const resizeObserver = new ResizeObserver(() => fitTerminal(tabId));
    resizeObserver.observe(host);
    const context = {
      terminal,
      fitAddon,
      sessionId: null,
      websocket: null,
      connected: false,
      inputBuffer: "",
      inputCursorIndex: 0,
      aiLineMode: false,
      aiBusy: false,
      dataDisposable,
      resizeObserver,
    };
    terminalContextsRef.current.set(tabId, context);
    terminalWorkspace.contexts = terminalContextsRef.current;
    return context;
  }

  async function loadTerminalServers() {
    try {
      const items = await sshServerApi.list();
      setTerminalServers(items);
      setSelectedAlias((current) => current ?? items[0]?.alias);
    } catch (error) {
      message.error(getErrorMessage(error));
      setTerminalServers([]);
    }
  }

  useEffect(() => {
    void loadTerminalServers();
  }, []);

  useEffect(() => {
    terminalTabsRef.current = terminalTabs;
    terminalWorkspace.tabs = terminalTabs;
  }, [terminalTabs]);

  useEffect(() => {
    activeTerminalIdRef.current = activeTerminalId;
    terminalWorkspace.activeId = activeTerminalId;
  }, [activeTerminalId]);

  useEffect(() => {
    const current = terminalTabs.find((tab) => tab.id === activeTerminalId);
    const assistantMessages = normalizeTerminalAiMessages(current).filter((item) => item.role === "assistant");
    const lastAnswer = assistantMessages[assistantMessages.length - 1]?.content;
    setAiAnswer(lastAnswer ?? terminalAiDefaultAnswer());
  }, [activeTerminalId, terminalTabs]);

  useEffect(() => {
    if (terminalServers.length === 0) {
      return;
    }
    const params = new URLSearchParams(location.search);
    const serverAlias = params.get("server")?.trim();
    const shouldConnect = params.get("connect") === "1";
    if (!serverAlias || !shouldConnect) {
      return;
    }
    const requestKey = params.get("request") ?? location.search;
    if (autoOpenRequestRef.current === requestKey) {
      return;
    }
    if (!terminalServers.some((server) => server.alias === serverAlias)) {
      message.warning(`服务器 ${serverAlias} 不存在或已被删除`);
      autoOpenRequestRef.current = requestKey;
      terminalWorkspace.handledRequestKey = requestKey;
      return;
    }
    autoOpenRequestRef.current = requestKey;
    terminalWorkspace.handledRequestKey = requestKey;
    setSelectedAlias(serverAlias);
    void openTerminalTab(serverAlias);
  }, [location.search, terminalServers]);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      return undefined;
    }
    let disposed = false;
    listen<TerminalSessionEvent>("terminal-session-event", (event) => {
      if (!disposed) {
        handleTerminalEvent(event.payload);
      }
    }).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        unlistenRef.current = unlisten;
      }
    }).catch((error) => message.error(getErrorMessage(error)));
    return () => {
      disposed = true;
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
  }, []);

  useEffect(() => {
    terminalTabs.forEach((tab) => {
      ensureTerminal(tab.id);
    });
    if (activeTerminalId) {
      setTimeout(() => fitTerminal(activeTerminalId), 0);
    }
  }, [terminalTabs, activeTerminalId]);

  useEffect(() => () => {
    terminalWorkspace.tabs = terminalTabsRef.current;
    terminalWorkspace.activeId = activeTerminalIdRef.current;
    terminalWorkspace.seq = terminalSeqRef.current;
    terminalWorkspace.handledRequestKey = autoOpenRequestRef.current;
    terminalWorkspace.hosts.clear();
  }, []);

  async function connectTerminalTab(tabId: string) {
    const tab = terminalTabsRef.current.find((item) => item.id === tabId);
    if (!tab) {
      return;
    }
    const context = ensureTerminal(tabId);
    if (!context || tab.connecting || tab.connected) {
      return;
    }
    const cols = context.terminal.cols || 100;
    const rows = context.terminal.rows || 30;
    updateTerminalTab(tabId, {
      connecting: true,
      risk: "safe",
      transcript: [],
      status: "正在建立 SSH 终端会话...",
    });
    context.terminal.clear();
    context.terminal.writeln(`\x1b[32m正在连接 ${tab.serverAlias} ...\x1b[0m`);
    try {
      if (hasTauriRuntime()) {
        const result = await terminalApi.startSession({
          serverAlias: tab.serverAlias,
          cols,
          rows,
        });
        context.sessionId = result.sessionId;
        updateTerminalTab(tabId, {
          connecting: true,
          connected: false,
          status: "SSH 终端会话已创建，正在后台连接服务器...",
        });
      } else {
        const websocket = new WebSocket(terminalApi.devWebSocketUrl({
          serverAlias: tab.serverAlias,
          cols,
          rows,
        }));
        context.websocket = websocket;
        websocket.onmessage = (event) => {
          try {
            const payload = JSON.parse(event.data) as TerminalSessionEvent;
            if (payload.sessionId && !context.sessionId) {
              context.sessionId = payload.sessionId;
            }
            const routedPayload = {
              ...payload,
              sessionId: context.sessionId ?? payload.sessionId,
            };
            handleTerminalEvent(routedPayload);
          } catch {
            context.terminal.writeln("\r\n\x1b[31m终端事件解析失败\x1b[0m");
          }
        };
        websocket.onerror = () => {
          updateTerminalTab(tabId, { status: "Dev WebSocket 终端连接失败", risk: "review", connected: false, connecting: false });
          context.terminal.writeln("\r\n\x1b[31mDev WebSocket 终端连接失败\x1b[0m");
        };
        websocket.onclose = () => {
          if (context.connected) {
            updateTerminalTab(tabId, { status: "Dev WebSocket 终端连接已关闭", connected: false, connecting: false });
            context.connected = false;
            context.terminal.writeln("\r\n\x1b[33mDev WebSocket 终端连接已关闭\x1b[0m");
          }
        };
        await new Promise<void>((resolve, reject) => {
          websocket.onopen = () => resolve();
          websocket.onerror = () => reject(new Error("Dev WebSocket 终端连接失败"));
        });
      }
      fitTerminal(tabId);
    } catch (error) {
      const errorMessage = getErrorMessage(error);
      updateTerminalTab(tabId, { status: errorMessage, connecting: false, connected: false, risk: "review" });
      context.connected = false;
      context.terminal.writeln(`\r\n\x1b[31m${errorMessage}\x1b[0m`);
      message.error(errorMessage);
    }
  }

  async function disconnectTerminalTab(tabId: string, writeNotice = true) {
    const context = terminalContextsRef.current.get(tabId);
    if (!context) {
      updateTerminalTab(tabId, { connected: false, connecting: false });
      return;
    }
    const hadConnection = context.connected || Boolean(context.sessionId) || Boolean(context.websocket);
    const sessionId = context.sessionId;
    const websocket = context.websocket;
    context.sessionId = null;
    context.connected = false;
    context.websocket = null;
    updateTerminalTab(tabId, { connected: false, connecting: false });
    if (hasTauriRuntime() && sessionId) {
      await terminalApi.closeSession({ sessionId }).catch(() => {});
    }
    if (websocket && websocket.readyState === WebSocket.OPEN) {
      websocket.send(JSON.stringify({ type: "close" }));
      websocket.close();
    }
    if (hadConnection && writeNotice) {
      updateTerminalTab(tabId, { status: "SSH 终端会话已断开。" });
      context.terminal.writeln("\r\n\x1b[33mSSH 终端会话已断开。\x1b[0m");
    }
  }

  async function closeTerminalTab(tabId: string, updateState = true) {
    await disconnectTerminalTab(tabId, false);
    const context = terminalContextsRef.current.get(tabId);
    if (context) {
      context.dataDisposable.dispose();
      context.resizeObserver.disconnect();
      context.terminal.dispose();
      terminalContextsRef.current.delete(tabId);
    }
    terminalHostsRef.current.delete(tabId);
    if (updateState) {
      setTerminalTabs((prev) => {
        const next = prev.filter((tab) => tab.id !== tabId);
        if (activeTerminalIdRef.current === tabId) {
          const nextActiveId = next.length > 0 ? next[next.length - 1].id : undefined;
          activeTerminalIdRef.current = nextActiveId;
          terminalWorkspace.activeId = nextActiveId;
          setActiveTerminalId(nextActiveId);
        }
        terminalTabsRef.current = next;
        terminalWorkspace.tabs = next;
        return next;
      });
    }
  }

  async function openTerminalTab(targetAlias = selectedAlias) {
    if (!targetAlias) {
      message.warning("请先选择服务器");
      return;
    }
    const seq = terminalSeqRef.current + 1;
    terminalSeqRef.current = seq;
    terminalWorkspace.seq = seq;
    const server = terminalServers.find((item) => item.alias === targetAlias);
    const sameServerCount = terminalTabsRef.current.filter((tab) => tab.serverAlias === targetAlias).length + 1;
    const tabId = `terminal-${Date.now()}-${seq}`;
    const title = `${targetAlias}${sameServerCount > 1 ? ` #${sameServerCount}` : ""}`;
    setTerminalTabs((prev) => {
      const next: TerminalTabState[] = [
        ...prev,
        {
          id: tabId,
          title,
          serverAlias: targetAlias,
          status: server ? `${server.username}@${formatServerAddress(server)}` : "等待连接",
          connected: false,
          connecting: false,
          risk: "safe",
          transcript: [],
          aiMessages: [],
        },
      ];
      terminalTabsRef.current = next;
      terminalWorkspace.tabs = next;
      return next;
    });
    activeTerminalIdRef.current = tabId;
    terminalWorkspace.activeId = tabId;
    setActiveTerminalId(tabId);
    setTimeout(() => {
      ensureTerminal(tabId);
      void connectTerminalTab(tabId);
    }, 0);
  }

  async function handleAskAi() {
    const question = aiQuestion.trim();
    if (!question) {
      message.warning("请输入要问 AI 的问题");
      return;
    }
    if (!activeTab) {
      message.warning("请先打开一个终端标签");
      return;
    }
    setTerminalAiAsking(true);
    setAiAnswer("AI 思考中，正在结合当前终端输出和历史对话...");
    try {
      const result = await aiProviderApi.ask({
        prompt: buildTerminalAiConversationPrompt(activeTab, question),
        skillScope: "terminal",
        useSkillTrigger: true,
        systemPrompt: [
          "你是 Tauri SSH 的终端 AI 对话助手。",
          "你需要把“本标签历史 AI 对话”和“最近终端输出”视为同一个连续会话上下文。",
          "回答用户本轮问题时，优先承接前文，不要把多轮问题当作彼此无关的新会话。",
          "如果建议执行命令，必须说明命令风险；危险或写入类命令不要诱导自动执行。",
          "用中文、Markdown 格式输出，保持简洁可操作。",
        ].join("\n"),
      });
      const answer = result.answer.trim();
      appendTerminalAiExchange(activeTab.id, question, answer);
      setAiAnswer(answer);
      setAiQuestion("");
    } catch (error) {
      const errorMessage = getErrorMessage(error);
      setAiAnswer(`AI 调用失败：${errorMessage}`);
      message.error(errorMessage);
    } finally {
      setTerminalAiAsking(false);
    }
  }

  async function handleSaveTerminalExperience() {
    const answer = aiAnswer.trim();
    if (!answer || answer.startsWith("打开一个 SSH 终端后")) {
      message.warning("当前没有可沉淀的终端 AI 输出");
      return;
    }
    setTerminalExperienceSaving(true);
    try {
      const serverAlias = activeTab?.serverAlias ?? "未选择服务器";
      const recentTranscript = activeTab?.transcript.slice(-80).join("\n") ?? "";
      const experience = await aiSkillApi.upsertExperience({
        title: `终端经验：${serverAlias}`,
        symptom: [
          `服务器：${serverAlias}`,
          recentTranscript ? `最近终端上下文：\n${truncateText(recentTranscript, 8_000)}` : "",
        ].filter(Boolean).join("\n\n"),
        cause: "",
        solution: answer,
        scenario: "terminal",
        source: "ai",
        tags: ["terminal", "ssh", "ai", serverAlias].filter(Boolean),
        enabled: true,
      });
      message.success(`已沉淀经验：${experience.title}`);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setTerminalExperienceSaving(false);
    }
  }

  function handleClearTerminal(tabId = activeTerminalId) {
    if (!tabId) {
      return;
    }
    terminalContextsRef.current.get(tabId)?.terminal.clear();
    updateTerminalTab(tabId, { transcript: [], risk: "safe" });
  }

  function toggleTerminalMaximized() {
    setTerminalMaximized((value) => !value);
    setTimeout(() => fitTerminal(), 0);
  }

  return (
    <div className={`prototype-page ${terminalMaximized ? "prototype-page-terminal-maximized" : ""}`}>
      <PageHeader
        title="终端 + AI"
        description="终端旁路接入 AI 问答、命令解释、风险分级和审批提示。"
        actions={
          <Space>
            <Select
              style={{ width: 240 }}
              placeholder="选择服务器"
              value={selectedAlias}
              onChange={(value) => setSelectedAlias(value)}
              options={terminalServers.map((server) => ({
                value: server.alias,
                label: `${server.alias} (${server.groupName})`,
              }))}
            />
            <Button
              type="primary"
              loading={selectedAliasOpening}
              disabled={!selectedAlias}
              onClick={() => void openTerminalTab()}
            >
              打开终端
            </Button>
          </Space>
        }
      />
      {!terminalMaximized ? (
        <SectionGrid columns={4}>
          <Card size="small" title="连接状态">
            <Badge status={connectedCount > 0 ? "success" : "default"} text={`${connectedCount}/${terminalTabs.length} 已连接`} />
          </Card>
          <Card size="small" title="当前标签">
            <Text>{activeTab ? activeTab.title : "未打开终端"}</Text>
          </Card>
          <Card size="small" title="AI 权限级别">
            {activeServerPolicy ? (
              <Tag color={sshPolicyColor[activeServerPolicy]}>
                {sshPolicyLabel[activeServerPolicy] ?? activeServerPolicy}
              </Tag>
            ) : (
              <Tag>未选择服务器</Tag>
            )}
          </Card>
          <Card size="small" title="执行模式">
            <Tag color="green">xterm.js + SSH PTY</Tag>
          </Card>
        </SectionGrid>
      ) : null}
      <div className={terminalMaximized ? "prototype-terminal-maximized-layout" : "prototype-two-column"}>
        <div>
          <Card
            size="small"
            title="终端标签"
            extra={
              <Space>
                <Text type="secondary">{activeTab?.status ?? "未打开终端"}</Text>
                <Button
                  size="small"
                  title={terminalMaximized ? "退出最大化" : "最大化终端"}
                  icon={terminalMaximized ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
                  onClick={toggleTerminalMaximized}
                />
                <Button size="small" disabled={!activeTab} onClick={() => fitTerminal()}>适配大小</Button>
                <Button size="small" disabled={!activeTab} onClick={() => handleClearTerminal()}>清屏</Button>
                {activeTab?.connected ? (
                  <Button size="small" onClick={() => activeTerminalId && void disconnectTerminalTab(activeTerminalId)}>断开</Button>
                ) : (
                  <Button
                    size="small"
                    type="primary"
                    loading={activeTab?.connecting}
                    disabled={!activeTerminalId}
                    onClick={() => activeTerminalId && void connectTerminalTab(activeTerminalId)}
                  >
                    连接
                  </Button>
                )}
              </Space>
            }
          >
            {terminalTabs.length === 0 ? (
              <div className="prototype-xterm-empty">
                <Text type="secondary">请选择服务器并点击“打开终端”。每个标签会创建独立 SSH PTY 会话。</Text>
              </div>
            ) : (
              <Tabs
                type="editable-card"
                hideAdd
                destroyOnHidden={false}
                activeKey={activeTerminalId}
                onChange={(key) => {
                  activeTerminalIdRef.current = key;
                  terminalWorkspace.activeId = key;
                  setActiveTerminalId(key);
                  setTimeout(() => fitTerminal(key), 0);
                }}
                onEdit={(targetKey, action) => {
                  if (action === "remove" && typeof targetKey === "string") {
                    void closeTerminalTab(targetKey);
                  }
                }}
                items={terminalTabs.map((tab) => ({
                  key: tab.id,
                  label: (
                    <Space size={6}>
                      <Badge status={tab.connected ? "success" : tab.connecting ? "processing" : "default"} />
                      <span>{tab.title}</span>
                    </Space>
                  ),
                  children: (
                    <div
                      ref={(node) => {
                        if (node) {
                          terminalHostsRef.current.set(tab.id, node);
                          ensureTerminal(tab.id);
                        } else {
                          terminalHostsRef.current.delete(tab.id);
                        }
                      }}
                      className="prototype-xterm"
                    />
                  ),
                }))}
              />
            )}
          </Card>
        </div>
        {!terminalMaximized ? (
          <div className="flex flex-col gap-4">
            <AiInsightPanel
              title="AI 命令建议"
              tone={activeTab?.risk === "blocked" ? "warning" : "normal"}
              className="prototype-ai-command-card"
            >
              <div className="prototype-ai-command-body">
                <div className="flex justify-end">
                  <Button
                    size="small"
                    loading={terminalExperienceSaving}
                    disabled={!aiAnswer.trim() || aiAnswer.startsWith("打开一个 SSH 终端后")}
                    onClick={() => void handleSaveTerminalExperience()}
                  >
                    沉淀经验
                  </Button>
                </div>
                <div className="prototype-ai-answer-scroll">
                  <MarkdownAnswer content={aiAnswer} />
                </div>
                <Input.Search
                  value={aiQuestion}
                  placeholder="问 AI：解释上一条输出 / 下一步怎么排查"
                  enterButton="提问"
                  loading={terminalAiAsking}
                  onChange={(event) => setAiQuestion(event.target.value)}
                  onSearch={() => void handleAskAi()}
                />
              </div>
            </AiInsightPanel>
            <AiInsightPanel title="安全说明">
              <Paragraph>每个标签都是独立 SSH PTY，会真实连接对应服务器。输入内容会直接进入当前标签远端 Shell；关闭标签会关闭该标签的远端会话。</Paragraph>
              <CodeBlock>{`支持：同一服务器多开 / 不同服务器多开\n支持：Ctrl+C / Tab / 方向键 / vi 等交互程序\n桌面端：Tauri IPC + event\n浏览器调试：Dev WebSocket`}</CodeBlock>
            </AiInsightPanel>
          </div>
        ) : null}
      </div>
    </div>
  );
}

export function ApprovalPage() {
  const [approvalRows, setApprovalRows] = useState<ApprovalRequest[]>([]);
  const [approvalLoading, setApprovalLoading] = useState(false);
  const [approvalStatus, setApprovalStatus] = useState<ApprovalStatus | "all">("pending");
  const [approvalDrawerOpen, setApprovalDrawerOpen] = useState(false);
  const [approvalDetail, setApprovalDetail] = useState<ApprovalRequest | null>(null);
  const [approvalSaving, setApprovalSaving] = useState(false);
  const [approvalForm] = Form.useForm<CreateApprovalRequestInput>();

  const loadApprovals = useCallback(async () => {
    setApprovalLoading(true);
    try {
      const rows = await approvalApi.list({ status: approvalStatus, limit: 200 });
      setApprovalRows(rows);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setApprovalLoading(false);
    }
  }, [approvalStatus]);

  useEffect(() => {
    void loadApprovals();
  }, [loadApprovals]);

  const statusMeta: Record<string, { color: string; label: string }> = {
    pending: { color: "gold", label: "待审批" },
    approved: { color: "green", label: "已批准" },
    rejected: { color: "red", label: "已拒绝" },
    cancelled: { color: "default", label: "已取消" },
    expired: { color: "default", label: "已过期" },
  };

  const riskMeta: Record<string, { color: string; label: string }> = {
    readonly: { color: "blue", label: "只读" },
    L1: { color: "green", label: "低风险" },
    L2: { color: "orange", label: "中风险" },
    L3: { color: "red", label: "高风险" },
    review: { color: "gold", label: "需审核" },
    high: { color: "red", label: "高风险" },
    blocked: { color: "volcano", label: "禁止" },
  };

  const decideApproval = useCallback((record: ApprovalRequest, decision: "approved" | "rejected") => {
    const isApprove = decision === "approved";
    Modal.confirm({
      title: isApprove ? "确认批准该请求？" : "确认拒绝该请求？",
      content: (
        <Space direction="vertical" size={8} className="w-full">
          <Text type="secondary">{record.summary || record.reason || record.action}</Text>
          <Input.TextArea
            id={`approval-note-${record.id}`}
            rows={3}
            placeholder="填写审批备注（可选）"
          />
        </Space>
      ),
      okText: isApprove ? "批准" : "拒绝",
      okButtonProps: { danger: !isApprove },
      cancelText: "取消",
      async onOk() {
        const note = (document.getElementById(`approval-note-${record.id}`) as HTMLTextAreaElement | null)?.value ?? "";
        try {
          await approvalApi.decide({
            id: record.id,
            decision,
            note,
            decidedBy: "local-user",
          });
          message.success(isApprove ? "已批准审批请求" : "已拒绝审批请求");
          await loadApprovals();
        } catch (error) {
          message.error(getErrorMessage(error));
          throw error;
        }
      },
    });
  }, [loadApprovals]);

  const createApproval = useCallback(async () => {
    try {
      const values = await approvalForm.validateFields();
      setApprovalSaving(true);
      await approvalApi.create({
        ...values,
        payloadJson: values.payloadJson?.trim() || "{}",
        expiresAt: values.expiresAt?.trim() || null,
      });
      message.success("审批请求已创建");
      setApprovalDrawerOpen(false);
      approvalForm.resetFields();
      await loadApprovals();
    } catch (error) {
      if (typeof error === "object" && error !== null && "errorFields" in error) {
        return;
      }
      message.error(getErrorMessage(error));
    } finally {
      setApprovalSaving(false);
    }
  }, [approvalForm, loadApprovals]);

  const renderApprovalText = (value: string, options?: { code?: boolean; secondary?: boolean }) => {
    const text = value || "-";
    return (
      <Tooltip title={value || undefined}>
        <Text
          code={options?.code}
          type={options?.secondary ? "secondary" : undefined}
          style={{ whiteSpace: "normal", wordBreak: "break-word" }}
        >
          {text}
        </Text>
      </Tooltip>
    );
  };

  const approvalRealColumns: TableProps<ApprovalRequest>["columns"] = [
    {
      title: "状态",
      dataIndex: "status",
      width: 90,
      render: (value: string) => {
        const meta = statusMeta[value] ?? { color: "default", label: value };
        return <Tag color={meta.color}>{meta.label}</Tag>;
      },
    },
    {
      title: "来源",
      dataIndex: "source",
      width: 110,
      render: (value: string) => renderApprovalText(value),
    },
    {
      title: "请求方",
      dataIndex: "requester",
      width: 190,
      render: (value: string) => renderApprovalText(value),
    },
    {
      title: "服务器",
      dataIndex: "serverAlias",
      width: 150,
      render: (value: string) => renderApprovalText(value),
    },
    {
      title: "动作",
      dataIndex: "action",
      width: 190,
      render: (value: string) => renderApprovalText(value),
    },
    {
      title: "风险",
      dataIndex: "risk",
      width: 100,
      render: (value: string) => {
        const meta = riskMeta[value] ?? { color: "default", label: value };
        return <Tag color={meta.color}>{meta.label}</Tag>;
      },
    },
    {
      title: "命令 / 资源",
      key: "target",
      width: 310,
      render: (_: unknown, record: ApprovalRequest) => (
        <Space direction="vertical" size={2} style={{ maxWidth: 290 }}>
          {record.command ? renderApprovalText(record.command, { code: true }) : <Text type="secondary">-</Text>}
          {record.resource ? renderApprovalText(record.resource, { secondary: true }) : null}
        </Space>
      ),
    },
    {
      title: "原因",
      dataIndex: "reason",
      width: 280,
      render: (value: string) => renderApprovalText(value),
    },
    { title: "创建时间", dataIndex: "createdAt", width: 170 },
    {
      title: "决策人",
      dataIndex: "decidedBy",
      width: 150,
      render: (value: string) => renderApprovalText(value, { secondary: !value }),
    },
    {
      title: "操作",
      key: "actions",
      width: 170,
      fixed: "right" as const,
      render: (_: unknown, record: ApprovalRequest) => (
        <Space size={8} wrap>
          <Button
            size="small"
            aria-label={`查看审批请求 ${record.id} 详情`}
            icon={<Eye size={13} />}
            onClick={() => setApprovalDetail(record)}
          >
            详情
          </Button>
          {record.status === "pending" ? (
            <>
              <Button size="small" type="primary" onClick={() => decideApproval(record, "approved")}>批准</Button>
              <Button size="small" danger onClick={() => decideApproval(record, "rejected")}>拒绝</Button>
            </>
          ) : null}
        </Space>
      ),
    },
  ];

  return (
    <div className="prototype-page">
      <PageHeader title="审批队列" description="集中处理 AI、终端、MCP、SFTP 写入触发的风险审批。" />
      <Card
        title="审批请求"
        extra={(
          <Space>
            <Select
              value={approvalStatus}
              style={{ width: 120 }}
              onChange={(value) => setApprovalStatus(value)}
              options={[
                { value: "pending", label: "待审批" },
                { value: "all", label: "全部" },
                { value: "approved", label: "已批准" },
                { value: "rejected", label: "已拒绝" },
                { value: "cancelled", label: "已取消" },
                { value: "expired", label: "已过期" },
              ]}
            />
            <Button icon={<RefreshCw size={14} />} onClick={loadApprovals} loading={approvalLoading}>刷新</Button>
            <Button type="primary" onClick={() => setApprovalDrawerOpen(true)}>新建审批请求</Button>
          </Space>
        )}
      >
        <Table
          rowKey="id"
          columns={approvalRealColumns}
          dataSource={approvalRows}
          loading={approvalLoading}
          pagination={{ pageSize: 10, showSizeChanger: false }}
          scroll={{ x: 1910 }}
        />
      </Card>
      <Drawer
        title={approvalDetail ? `审批请求 #${approvalDetail.id}` : "审批请求详情"}
        open={Boolean(approvalDetail)}
        width={760}
        onClose={() => setApprovalDetail(null)}
        extra={<Button onClick={() => setApprovalDetail(null)}>关闭</Button>}
      >
        {approvalDetail ? (
          <Space direction="vertical" size={16} style={{ width: "100%" }}>
            <Descriptions bordered size="small" column={2}>
              <Descriptions.Item label="状态">
                {statusMeta[approvalDetail.status]?.label ?? approvalDetail.status}
              </Descriptions.Item>
              <Descriptions.Item label="风险">
                {riskMeta[approvalDetail.risk]?.label ?? approvalDetail.risk}
              </Descriptions.Item>
              <Descriptions.Item label="来源">{approvalDetail.source || "-"}</Descriptions.Item>
              <Descriptions.Item label="请求方">{approvalDetail.requester || "-"}</Descriptions.Item>
              <Descriptions.Item label="服务器">{approvalDetail.serverAlias || "-"}</Descriptions.Item>
              <Descriptions.Item label="动作">{approvalDetail.action || "-"}</Descriptions.Item>
              <Descriptions.Item label="创建时间">{approvalDetail.createdAt || "-"}</Descriptions.Item>
              <Descriptions.Item label="更新时间">{approvalDetail.updatedAt || "-"}</Descriptions.Item>
              <Descriptions.Item label="决策人">{approvalDetail.decidedBy || "-"}</Descriptions.Item>
              <Descriptions.Item label="决策时间">{approvalDetail.decidedAt || "-"}</Descriptions.Item>
            </Descriptions>
            <div>
              <Text strong>摘要</Text>
              <Paragraph style={{ marginTop: 8 }}>{approvalDetail.summary || "-"}</Paragraph>
            </div>
            <div>
              <Text strong>命令</Text>
              <pre className="prototype-code" style={{ maxHeight: 160, overflow: "auto", marginTop: 8 }}>
                {approvalDetail.command || "-"}
              </pre>
            </div>
            <div>
              <Text strong>资源</Text>
              <Paragraph code copyable={Boolean(approvalDetail.resource)} style={{ marginTop: 8 }}>
                {approvalDetail.resource || "-"}
              </Paragraph>
            </div>
            <div>
              <Text strong>申请原因</Text>
              <Paragraph style={{ marginTop: 8 }}>{approvalDetail.reason || "-"}</Paragraph>
            </div>
            <div>
              <Text strong>审批备注</Text>
              <Paragraph style={{ marginTop: 8 }}>{approvalDetail.decisionNote || "-"}</Paragraph>
            </div>
            <div>
              <Text strong>Payload JSON</Text>
              <pre className="prototype-code" style={{ maxHeight: 260, overflow: "auto", marginTop: 8 }}>
                {approvalDetail.payloadJson || "{}"}
              </pre>
            </div>
          </Space>
        ) : null}
      </Drawer>
      <Drawer
        title="新建审批请求"
        open={approvalDrawerOpen}
        width={560}
        onClose={() => setApprovalDrawerOpen(false)}
        extra={(
          <Space>
            <Button onClick={() => setApprovalDrawerOpen(false)}>取消</Button>
            <Button type="primary" loading={approvalSaving} onClick={createApproval}>提交</Button>
          </Space>
        )}
      >
        <Form<CreateApprovalRequestInput>
          form={approvalForm}
          layout="vertical"
          initialValues={{
            source: "terminal_ai",
            requester: "local-user",
            risk: "L2",
            action: "terminal_execute",
            payloadJson: "{}",
          }}
        >
          <Form.Item name="source" label="来源" rules={[{ required: true, message: "请选择来源" }]}>
            <Select options={[
              { value: "terminal_ai", label: "终端 AI" },
              { value: "mcp", label: "MCP Agent" },
              { value: "sftp", label: "SFTP" },
              { value: "user", label: "本机用户" },
            ]} />
          </Form.Item>
          <Form.Item name="requester" label="请求方" rules={[{ required: true, message: "请输入请求方" }]}>
            <Input placeholder="例如 Codex / Claude Code / local-user" />
          </Form.Item>
          <Form.Item name="serverAlias" label="服务器别名">
            <Input placeholder="例如 bailing-dev-71" />
          </Form.Item>
          <Form.Item name="action" label="动作" rules={[{ required: true, message: "请输入动作" }]}>
            <Select options={[
              { value: "terminal_execute", label: "执行命令" },
              { value: "sftp_write_text", label: "写入文件" },
              { value: "sftp_upload", label: "上传文件" },
              { value: "server_config_change", label: "修改服务器配置" },
            ]} />
          </Form.Item>
          <Form.Item name="risk" label="AI 权限级别 / 风险" rules={[{ required: true, message: "请选择风险级别" }]}>
            <Select options={[
              { value: "readonly", label: "只读" },
              { value: "L1", label: "低风险" },
              { value: "L2", label: "中风险" },
              { value: "L3", label: "高风险" },
              { value: "review", label: "需审核" },
              { value: "blocked", label: "禁止" },
            ]} />
          </Form.Item>
          <Form.Item name="command" label="命令">
            <Input.TextArea rows={3} placeholder="需要审批的远程命令" />
          </Form.Item>
          <Form.Item name="resource" label="资源路径">
            <Input placeholder="例如 /etc/nginx/nginx.conf" />
          </Form.Item>
          <Form.Item name="summary" label="摘要">
            <Input placeholder="一句话说明请求内容" />
          </Form.Item>
          <Form.Item name="reason" label="申请原因">
            <Input.TextArea rows={3} placeholder="说明为什么需要执行该操作" />
          </Form.Item>
          <Form.Item name="payloadJson" label="Payload JSON">
            <Input.TextArea rows={4} />
          </Form.Item>
          <Form.Item name="expiresAt" label="过期时间">
            <Input placeholder="可选，例如 2026-06-20 18:00:00" />
          </Form.Item>
        </Form>
      </Drawer>
    </div>
  );
}

export function LogsPage() {
  const [logServerList, setLogServerList] = useState<SshServer[]>([]);
  const [logTabsState, setLogTabsState] = useState<LogWatchTabState[]>([]);
  const [activeLogTabId, setActiveLogTabId] = useState<string>();
  const [logModalOpen, setLogModalOpen] = useState(false);
  const [logAiAnswer, setLogAiAnswer] = useState("");
  const [logAiLoading, setLogAiLoading] = useState(false);
  const [logExperienceSaving, setLogExperienceSaving] = useState(false);
  const logTabsRef = useRef<LogWatchTabState[]>([]);
  const [logForm] = Form.useForm();

  useEffect(() => {
    logTabsRef.current = logTabsState;
  }, [logTabsState]);

  const updateLogTab = useCallback((id: string, patch: Partial<LogWatchTabState>) => {
    setLogTabsState((prev) => prev.map((tab) => (tab.id === id ? { ...tab, ...patch } : tab)));
  }, []);

  const loadLogServers = useCallback(async () => {
    try {
      const list = await sshServerApi.list();
      const usable = list.filter((server) => server.enabled && server.source !== "jumpserver" && server.authType !== "session_reference");
      setLogServerList(usable);
      logForm.setFieldsValue({ serverAlias: usable[0]?.alias });
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }, [logForm]);

  useEffect(() => {
    void loadLogServers();
  }, [loadLogServers]);

  const shellQuote = useCallback((value: string) => `'${value.replace(/'/g, "'\\''")}'`, []);

  const runLogTail = useCallback(async (id: string) => {
    const tab = logTabsRef.current.find((item) => item.id === id);
    if (!tab || tab.status === "paused" || tab.refreshing) {
      return;
    }
    const safeLineCount = Math.min(5000, Math.max(20, Number(tab.lineCount) || 200));
    updateLogTab(id, { refreshing: true, lastRunAt: Date.now(), error: null });
    try {
      const result = await terminalApi.execute({
        serverAlias: tab.serverAlias,
        command: `tail -n ${safeLineCount} ${shellQuote(tab.filePath)}`,
        timeoutSecs: 20,
      });
      const raw = [result.stdout, result.stderr].filter(Boolean).join(result.stdout && result.stderr ? "\n" : "").trimEnd();
      const lines = raw ? raw.replace(/\r\n/g, "\n").split("\n") : [];
      updateLogTab(id, {
        raw,
        lines,
        status: result.exitStatus === 0 ? "tailing" : "error",
        error: result.exitStatus === 0 ? null : result.message || result.stderr || `tail 退出码 ${result.exitStatus}`,
        lastUpdatedAt: new Date().toLocaleString(),
        refreshing: false,
      });
    } catch (error) {
      updateLogTab(id, {
        status: "error",
        error: getErrorMessage(error),
        refreshing: false,
        lastUpdatedAt: new Date().toLocaleString(),
      });
    }
  }, [shellQuote, updateLogTab]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      const now = Date.now();
      logTabsRef.current.forEach((tab) => {
        if (tab.status !== "tailing" || tab.refreshing) {
          return;
        }
        if (now - tab.lastRunAt >= Math.max(2, tab.intervalSecs) * 1000) {
          void runLogTail(tab.id);
        }
      });
    }, 1000);
    return () => window.clearInterval(timer);
  }, [runLogTail]);

  const closeLogTab = useCallback((id: string) => {
    setLogTabsState((prev) => {
      const next = prev.filter((tab) => tab.id !== id);
      if (activeLogTabId === id) {
        setActiveLogTabId(next[next.length - 1]?.id);
      }
      return next;
    });
  }, [activeLogTabId]);

  const filterLogLines = useCallback((tab: LogWatchTabState) => {
    const keyword = tab.keyword.trim();
    if (!keyword) {
      return tab.lines;
    }
    let matcher: (line: string) => boolean;
    if (tab.regex) {
      try {
        const pattern = new RegExp(keyword, tab.caseSensitive ? "" : "i");
        matcher = (line) => pattern.test(line);
      } catch {
        matcher = () => false;
      }
    } else {
      const expected = tab.caseSensitive ? keyword : keyword.toLowerCase();
      matcher = (line) => (tab.caseSensitive ? line : line.toLowerCase()).includes(expected);
    }
    return tab.lines.filter((line) => (tab.inverse ? !matcher(line) : matcher(line)) || !tab.onlyMatches);
  }, []);

  const activeLogTab = useMemo(
    () => logTabsState.find((tab) => tab.id === activeLogTabId) ?? logTabsState[0],
    [activeLogTabId, logTabsState],
  );

  async function handleAddLogWatch(values: Record<string, unknown>) {
    const serverAlias = String(values.serverAlias ?? "").trim();
    const filePath = String(values.filePath ?? "").trim();
    if (!serverAlias || !filePath) {
      message.warning("请选择服务器并填写日志文件路径");
      return;
    }
    const id = `log-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    const title = filePath.split("/").filter(Boolean).pop() || filePath;
    const nextTab: LogWatchTabState = {
      id,
      title,
      serverAlias,
      filePath,
      lineCount: Number(values.lineCount ?? 200),
      intervalSecs: Number(values.intervalSecs ?? 5),
      keyword: String(values.keyword ?? ""),
      onlyMatches: Boolean(values.onlyMatches ?? false),
      regex: Boolean(values.regex ?? false),
      caseSensitive: Boolean(values.caseSensitive ?? false),
      inverse: Boolean(values.inverse ?? false),
      status: "tailing",
      raw: "",
      lines: [],
      error: null,
      lastUpdatedAt: null,
      lastRunAt: 0,
      refreshing: false,
    };
    setLogTabsState((prev) => [...prev, nextTab]);
    setActiveLogTabId(id);
    setLogModalOpen(false);
    logForm.setFieldsValue({ filePath: "", keyword: "" });
    window.setTimeout(() => void runLogTail(id), 0);
  }

  async function handleExplainLog(tab = activeLogTab) {
    if (!tab) {
      message.warning("请先添加日志监听");
      return;
    }
    const linesForAi = filterLogLines(tab).slice(-200).join("\n").trim();
    if (!linesForAi) {
      message.warning("当前标签没有可解释的日志内容");
      return;
    }
    setLogAiLoading(true);
    setLogAiAnswer("AI 思考中...");
    try {
      const result = await aiProviderApi.ask({
        prompt: [
          `服务器：${tab.serverAlias}`,
          `日志文件：${tab.filePath}`,
          `过滤关键词：${tab.keyword || "无"}`,
          `最近日志：\n${truncateText(linesForAi, 12_000)}`,
        ].join("\n\n"),
        skillScope: "logs",
        useSkillTrigger: true,
        systemPrompt: "你是日志分析助手。请用中文基于真实日志内容解释异常、归纳可能原因，并给出下一步只读排查建议。不要编造日志中不存在的事实。",
      });
      setLogAiAnswer(`### ${result.providerName} / ${result.model}\n\n${result.answer}`);
    } catch (error) {
      setLogAiAnswer("");
      message.error(getErrorMessage(error));
    } finally {
      setLogAiLoading(false);
    }
  }

  async function handleSaveLogExperience() {
    const answer = logAiAnswer.trim();
    if (!activeLogTab || !answer || answer === "AI 思考中...") {
      message.warning("当前没有可沉淀的日志 AI 输出");
      return;
    }
    setLogExperienceSaving(true);
    try {
      const visibleLines = filterLogLines(activeLogTab).slice(-200).join("\n");
      const experience = await aiSkillApi.upsertExperience({
        title: `日志经验：${activeLogTab.title}`,
        symptom: [
          `服务器：${activeLogTab.serverAlias}`,
          `日志文件：${activeLogTab.filePath}`,
          `过滤关键词：${activeLogTab.keyword || "无"}`,
          `最近日志：\n${truncateText(visibleLines, 10_000)}`,
        ].join("\n\n"),
        cause: "",
        solution: answer,
        scenario: "logs",
        source: "ai",
        tags: ["logs", "tail", "ai", activeLogTab.serverAlias].filter(Boolean),
        enabled: true,
      });
      message.success(`已沉淀经验：${experience.title}`);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLogExperienceSaving(false);
    }
  }

  const statusMeta: Record<LogWatchStatus, { text: string; color: string; badge: "success" | "default" | "error" }> = {
    tailing: { text: "监听中", color: "green", badge: "success" },
    paused: { text: "已暂停", color: "default", badge: "default" },
    error: { text: "异常", color: "red", badge: "error" },
  };

  return (
    <div className="prototype-page">
      <PageHeader
        title="日志监听"
        description="标签多开 tail，同一或不同服务器多个文件并发监听，支持搜索、过滤和按钮触发 AI 解释。"
        actions={
          <Space>
            <Button onClick={() => void loadLogServers()}>刷新服务器</Button>
            <Button type="primary" onClick={() => setLogModalOpen(true)}>添加监听</Button>
          </Space>
        }
      />
      <Card
        className="prototype-logs-tail-card"
        title="多标签 Tail"
        extra={<Text type="secondary">{logTabsState.length} 个监听标签</Text>}
      >
        {logTabsState.length === 0 ? (
          <div className="prototype-log-empty">
            <Text type="secondary">暂无日志监听，点击右上角“添加监听”选择服务器和日志文件。</Text>
          </div>
        ) : (
          <Tabs
            activeKey={activeLogTab?.id}
            onChange={setActiveLogTabId}
            items={logTabsState.map((tab) => {
              const visibleLines = filterLogLines(tab);
              const meta = statusMeta[tab.status];
              return {
                key: tab.id,
                label: (
                  <Space size={6}>
                    <Badge status={tab.refreshing ? "processing" : meta.badge} />
                    <span>{tab.title}</span>
                    <Tag color={meta.color}>{meta.text}</Tag>
                  </Space>
                ),
                children: (
                  <div className="prototype-log-tab">
                    <div className="prototype-log-toolbar">
                      <Space className="prototype-log-actions" wrap>
                        <Tag>{tab.serverAlias}</Tag>
                        <Text code>{tab.filePath}</Text>
                        <Text type="secondary">最近 {tab.lineCount} 行 / {tab.intervalSecs}s</Text>
                        {tab.lastUpdatedAt ? <Text type="secondary">更新：{tab.lastUpdatedAt}</Text> : null}
                      </Space>
                      <Space wrap>
                        <Input.Search
                          className="prototype-log-search"
                          value={tab.keyword}
                          allowClear
                          placeholder="关键词 / 正则"
                          onChange={(event) => updateLogTab(tab.id, { keyword: event.target.value })}
                        />
                        <Checkbox checked={tab.onlyMatches} onChange={(event) => updateLogTab(tab.id, { onlyMatches: event.target.checked })}>仅匹配</Checkbox>
                        <Checkbox checked={tab.regex} onChange={(event) => updateLogTab(tab.id, { regex: event.target.checked })}>正则</Checkbox>
                        <Checkbox checked={tab.caseSensitive} onChange={(event) => updateLogTab(tab.id, { caseSensitive: event.target.checked })}>大小写</Checkbox>
                        <Checkbox checked={tab.inverse} onChange={(event) => updateLogTab(tab.id, { inverse: event.target.checked })}>反向</Checkbox>
                        <Button className="prototype-log-action-btn" onClick={() => updateLogTab(tab.id, { status: tab.status === "paused" ? "tailing" : "paused" })}>
                          {tab.status === "paused" ? "继续" : "暂停"}
                        </Button>
                        <Button className="prototype-log-action-btn" disabled={tab.refreshing} onClick={() => void runLogTail(tab.id)}>
                          <span className="prototype-log-action-content">
                            <span className="prototype-log-action-icon-slot">{tab.refreshing ? <span className="prototype-log-button-spinner" /> : null}</span>
                            <span>刷新</span>
                          </span>
                        </Button>
                        <Button className="prototype-log-action-btn" disabled={logAiLoading && activeLogTab?.id === tab.id} onClick={() => { setActiveLogTabId(tab.id); void handleExplainLog(tab); }}>
                          AI 解释
                        </Button>
                        <Button className="prototype-log-action-btn" danger onClick={() => closeLogTab(tab.id)}>关闭</Button>
                      </Space>
                    </div>
                    {tab.error ? <Alert className="prototype-log-error" type="error" showIcon message={tab.error} /> : null}
                    <pre className="prototype-log-tail-output">
                      {visibleLines.length > 0 ? visibleLines.join("\n") : "等待日志输出..."}
                    </pre>
                  </div>
                ),
              };
            })}
          />
        )}
      </Card>
      <AiInsightPanel title="AI 日志解释">
        {logAiAnswer && logAiAnswer !== "AI 思考中..." ? (
          <div className="flex justify-end">
            <Button size="small" loading={logExperienceSaving} onClick={() => void handleSaveLogExperience()}>
              沉淀经验
            </Button>
          </div>
        ) : null}
        {logAiLoading ? (
          <Paragraph>AI 思考中...</Paragraph>
        ) : logAiAnswer ? (
          <MarkdownAnswer content={logAiAnswer} />
        ) : (
          <Paragraph>选择一个日志标签后点击“AI 解释”，系统会将当前过滤后的最近 200 行日志发送给已配置的 AI Provider 分析。</Paragraph>
        )}
      </AiInsightPanel>
      <Modal
        title="添加日志监听"
        open={logModalOpen}
        okText="开始监听"
        cancelText="取消"
        destroyOnHidden
        onCancel={() => setLogModalOpen(false)}
        onOk={() => void logForm.validateFields().then(handleAddLogWatch)}
      >
        <Form
          form={logForm}
          layout="vertical"
          initialValues={{ lineCount: 200, intervalSecs: 5, onlyMatches: false, regex: false, caseSensitive: false, inverse: false }}
        >
          <Form.Item name="serverAlias" label="服务器" rules={[{ required: true, message: "请选择服务器" }]}>
            <Select
              placeholder="选择已配置服务器"
              options={logServerList.map((server) => ({
                value: server.alias,
                label: `${server.alias} · ${server.username}@${formatServerAddress(server)}`,
              }))}
            />
          </Form.Item>
          <Form.Item name="filePath" label="日志文件路径" rules={[{ required: true, message: "请填写日志文件路径" }]}>
            <Input placeholder="/opt/app/logs/app.log" />
          </Form.Item>
          <SectionGrid columns={2}>
            <Form.Item name="lineCount" label="读取行数" rules={[{ required: true, message: "请填写读取行数" }]}>
              <Input type="number" min={20} max={5000} />
            </Form.Item>
            <Form.Item name="intervalSecs" label="刷新间隔（秒）" rules={[{ required: true, message: "请填写刷新间隔" }]}>
              <Input type="number" min={2} max={60} />
            </Form.Item>
          </SectionGrid>
          <Form.Item name="keyword" label="初始关键词">
            <Input placeholder="可留空" />
          </Form.Item>
          <Space wrap>
            <Form.Item name="onlyMatches" valuePropName="checked" noStyle><Checkbox>仅显示匹配行</Checkbox></Form.Item>
            <Form.Item name="regex" valuePropName="checked" noStyle><Checkbox>正则过滤</Checkbox></Form.Item>
            <Form.Item name="caseSensitive" valuePropName="checked" noStyle><Checkbox>大小写敏感</Checkbox></Form.Item>
            <Form.Item name="inverse" valuePropName="checked" noStyle><Checkbox>反向过滤</Checkbox></Form.Item>
          </Space>
        </Form>
      </Modal>
    </div>
  );
}

export function SftpPage() {
  type SftpActionType = "upload" | "uploadFolder" | "download" | "createFile" | "createDirectory" | "rename" | "chmod" | "delete";
  const [serverList, setServerList] = useState<SshServer[]>([]);
  const [selectedServerAlias, setSelectedServerAlias] = useState<string>();
  const [currentPath, setCurrentPath] = useState(".");
  const [pathInput, setPathInput] = useState(".");
  const [entries, setEntries] = useState<SftpFileEntry[]>([]);
  const [parentPath, setParentPath] = useState(".");
  const [loading, setLoading] = useState(false);
  const [filterText, setFilterText] = useState("");
  const [selectedRowKeys, setSelectedRowKeys] = useState<React.Key[]>([]);
  const [pathHistory, setPathHistory] = useState<string[]>(["."]);
  const [pathHistoryIndex, setPathHistoryIndex] = useState(0);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editorPath, setEditorPath] = useState("");
  const [editorContent, setEditorContent] = useState("");
  const [editorLoading, setEditorLoading] = useState(false);
  const [actionOpen, setActionOpen] = useState(false);
  const [actionType, setActionType] = useState<SftpActionType>("upload");
  const [actionEntry, setActionEntry] = useState<SftpFileEntry | null>(null);
  const [actionValues, setActionValues] = useState({ name: "", localPath: "", remotePath: "" });

  const selectedServer = useMemo(
    () => serverList.find((server) => server.alias === selectedServerAlias),
    [serverList, selectedServerAlias],
  );

  const filteredEntries = useMemo(() => {
    const keyword = filterText.trim().toLowerCase();
    if (!keyword) {
      return entries;
    }
    return entries.filter((entry) => (
      entry.name.toLowerCase().includes(keyword)
      || entry.path.toLowerCase().includes(keyword)
      || entry.permissions.toLowerCase().includes(keyword)
    ));
  }, [entries, filterText]);

  const editorLanguage = useMemo(() => getSftpEditorLanguage(editorPath), [editorPath]);
  const editorLineCount = useMemo(() => editorContent ? editorContent.split(/\r\n|\r|\n/).length : 1, [editorContent]);

  async function loadServers() {
    try {
      const list = await sshServerApi.list();
      const usable = list.filter((server) => server.enabled && server.source !== "jumpserver" && server.authType !== "session_reference");
      setServerList(usable);
      setSelectedServerAlias((current) => current ?? usable[0]?.alias);
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function loadDirectory(path = currentPath, serverAlias = selectedServerAlias, recordHistory = true) {
    if (!serverAlias) {
      setEntries([]);
      return;
    }
    setLoading(true);
    try {
      const result = await sftpApi.list({ serverAlias, path });
      setEntries(result.entries);
      setCurrentPath(result.path);
      setPathInput(result.path);
      setParentPath(result.parent);
      setSelectedRowKeys([]);
      if (recordHistory) {
        setPathHistory((prev) => {
          const nextBase = prev.slice(0, pathHistoryIndex + 1);
          if (nextBase[nextBase.length - 1] === result.path) {
            return nextBase;
          }
          const next = [...nextBase, result.path];
          setPathHistoryIndex(next.length - 1);
          return next;
        });
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }

  async function loadHistoryPath(nextIndex: number) {
    const target = pathHistory[nextIndex];
    if (!target) {
      return;
    }
    setPathHistoryIndex(nextIndex);
    await loadDirectory(target, selectedServerAlias, false);
  }

  useEffect(() => {
    void loadServers();
  }, []);

  useEffect(() => {
    if (selectedServerAlias) {
      void loadDirectory(".", selectedServerAlias);
    }
  }, [selectedServerAlias]);

  async function openTextEditor(entry: SftpFileEntry) {
    if (!selectedServerAlias || entry.fileType === "directory") {
      return;
    }
    setEditorOpen(true);
    setEditorPath(entry.path);
    setEditorLoading(true);
    try {
      const result = await sftpApi.readText({ serverAlias: selectedServerAlias, path: entry.path, maxBytes: 1024 * 1024 });
      setEditorContent(result.content);
      if (result.truncated) {
        message.warning("文件超过读取限制，已截断显示");
      }
    } catch (error) {
      message.error(getErrorMessage(error));
      setEditorOpen(false);
    } finally {
      setEditorLoading(false);
    }
  }

  async function saveEditor() {
    if (!selectedServerAlias || !editorPath) {
      return;
    }
    setEditorLoading(true);
    try {
      const result = await sftpApi.writeText({ serverAlias: selectedServerAlias, path: editorPath, content: editorContent });
      message.success(result.message);
      await loadDirectory();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setEditorLoading(false);
    }
  }

  function openAction(type: SftpActionType, entry: SftpFileEntry | null = null) {
    setActionType(type);
    setActionEntry(entry);
    setActionValues({
      name: type === "chmod" ? "755" : entry?.name ?? "",
      localPath: "",
      remotePath: entry?.path ?? joinSftpPath(currentPath, ""),
    });
    setActionOpen(true);
  }

  async function confirmAction() {
    if (!selectedServerAlias) {
      return;
    }
    try {
      let result;
      if (actionType === "createFile") {
        result = await sftpApi.createFile({
          serverAlias: selectedServerAlias,
          path: joinSftpPath(currentPath, actionValues.name),
          content: "",
        });
      } else if (actionType === "createDirectory") {
        result = await sftpApi.createDirectory({
          serverAlias: selectedServerAlias,
          path: joinSftpPath(currentPath, actionValues.name),
        });
      } else if (actionType === "rename" && actionEntry) {
        result = await sftpApi.rename({
          serverAlias: selectedServerAlias,
          fromPath: actionEntry.path,
          toPath: joinSftpPath(actionEntry.parent, actionValues.name),
        });
      } else if (actionType === "chmod" && actionEntry) {
        const mode = actionValues.name.trim();
        if (!/^[0-7]{3,4}$/.test(mode)) {
          message.warning("权限模式需为 755、644 这类 3-4 位八进制数字");
          return;
        }
        result = await terminalApi.execute({
          serverAlias: selectedServerAlias,
          command: `chmod ${mode} ${JSON.stringify(actionEntry.path)}`,
          timeoutSecs: 20,
        });
        if (result.exitStatus !== 0) {
          throw new Error(result.stderr || result.stdout || result.message || "权限修改失败");
        }
      } else if (actionType === "delete" && actionEntry) {
        result = await sftpApi.delete({
          serverAlias: selectedServerAlias,
          path: actionEntry.path,
          fileType: actionEntry.fileType,
        });
      } else if (actionType === "upload") {
        result = await sftpApi.upload({
          serverAlias: selectedServerAlias,
          localPath: actionValues.localPath,
          remotePath: actionValues.remotePath || joinSftpPath(currentPath, actionValues.name),
        });
      } else if (actionType === "uploadFolder") {
        throw new Error("当前后端仅支持单文件上传，递归上传文件夹接口尚未接入");
      } else if (actionType === "download" && actionEntry) {
        result = await sftpApi.download({
          serverAlias: selectedServerAlias,
          remotePath: actionEntry.path,
          localPath: actionValues.localPath,
        });
      }
      if (result) {
        message.success("message" in result ? result.message : "操作已完成");
      }
      setActionOpen(false);
      await loadDirectory();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function copyPath(entry: SftpFileEntry) {
    try {
      await navigator.clipboard.writeText(entry.path);
      message.success("路径已复制");
    } catch {
      message.info(entry.path);
    }
  }

  function openEntry(entry: SftpFileEntry) {
    if (entry.fileType === "directory") {
      void loadDirectory(entry.path);
    } else {
      void openTextEditor(entry);
    }
  }

  const actionTitle: Record<SftpActionType, string> = {
    upload: "上传本地文件",
    uploadFolder: "上传本地文件夹",
    download: "下载远程文件",
    createFile: "新建远程文件",
    createDirectory: "新建远程目录",
    rename: "重命名远程路径",
    chmod: "修改远程权限",
    delete: "删除远程路径",
  };

  const toolbarButtonStyle = { height: 28 };
  const iconButtonStyle = { width: 28, height: 28 };

  return (
    <div className="prototype-page prototype-sftp-page">
      <PageHeader
        title="文件传输（SFTP）"
        description="浏览 / 分块流式上传与下载（无大小限制）/ 新建 / 重命名 / 删除 / 压缩解压 / 在线编辑。AI 经 MCP 调 sftp_* 时对敏感路径强制拦截 ✅"
      />
      <div className="prototype-sftp-server-strip">
        <Select
          size="small"
          className="prototype-sftp-server-select"
          placeholder="选择服务器"
          value={selectedServerAlias}
          options={serverList.map((server) => ({ value: server.alias, label: `${server.alias} · ${server.host}` }))}
          onChange={setSelectedServerAlias}
        />
        <Tag>{selectedServer ? `${selectedServer.host}:${selectedServer.port}` : "未选择"}</Tag>
        <Tag>{selectedServer?.groupName ?? "未分组"}</Tag>
        {selectedServer ? (
          <Tag color={(sshServerStatusMeta[selectedServer.status] ?? sshServerStatusMeta.unknown).color}>
            {(sshServerStatusMeta[selectedServer.status] ?? sshServerStatusMeta.unknown).text}
          </Tag>
        ) : null}
      </div>
      {!selectedServer ? (
        <Alert
          showIcon
          type="warning"
          title="没有可用的直连 SSH 服务器"
          description="请先在服务器管理中新增启用的非 JumpServer 服务器，并配置直接密码、密码引用或密钥文件。"
        />
      ) : null}
      <Card className="prototype-sftp-shell" styles={{ body: { padding: 0 } }}>
        <div className="prototype-sftp-nav">
          <Space size={6}>
            <Button size="small" icon={<ChevronLeft size={14} />} style={iconButtonStyle} disabled={pathHistoryIndex <= 0} onClick={() => void loadHistoryPath(pathHistoryIndex - 1)} />
            <Button size="small" icon={<ChevronRight size={14} />} style={iconButtonStyle} disabled={pathHistoryIndex >= pathHistory.length - 1} onClick={() => void loadHistoryPath(pathHistoryIndex + 1)} />
            <Button size="small" icon={<ChevronsUp size={14} />} style={iconButtonStyle} disabled={!selectedServerAlias || currentPath === parentPath} onClick={() => void loadDirectory(parentPath)} />
            <Button size="small" icon={<Home size={14} />} style={iconButtonStyle} disabled={!selectedServerAlias} onClick={() => void loadDirectory(".")} />
          </Space>
          <Space.Compact className="prototype-sftp-path">
            <Input size="small" value={pathInput} onChange={(event) => setPathInput(event.target.value)} onPressEnter={() => void loadDirectory(pathInput)} prefix={<Folder size={14} />} />
            <Button size="small" onClick={() => void loadDirectory(pathInput)} disabled={!selectedServerAlias}>打开</Button>
          </Space.Compact>
          <Space size={6}>
            <Button size="small" icon={<RefreshCw size={14} />} style={toolbarButtonStyle} onClick={() => void loadDirectory()} disabled={!selectedServerAlias}>最近</Button>
            <Button size="small" icon={<RefreshCw size={14} />} style={iconButtonStyle} onClick={() => void loadDirectory()} disabled={!selectedServerAlias} />
          </Space>
        </div>

        <div className="prototype-sftp-actions">
          <Space size={8}>
            <Button size="small" type="primary" icon={<Upload size={14} />} onClick={() => openAction("upload")} disabled={!selectedServerAlias}>上传文件</Button>
            <Button size="small" icon={<UploadCloud size={14} />} onClick={() => openAction("uploadFolder")} disabled={!selectedServerAlias}>上传文件夹</Button>
            <Button size="small" icon={<FolderPlus size={14} />} onClick={() => openAction("createDirectory")} disabled={!selectedServerAlias}>新建文件夹</Button>
            <Button size="small" icon={<FilePlus2 size={14} />} onClick={() => openAction("createFile")} disabled={!selectedServerAlias}>新建文件</Button>
          </Space>
          <div className="prototype-sftp-filter">
            <Input size="small" allowClear prefix={<Search size={14} />} placeholder="过滤" value={filterText} onChange={(event) => setFilterText(event.target.value)} />
            <Text type="secondary">{currentPath}</Text>
          </div>
        </div>

        <Table<SftpFileEntry>
          className="prototype-sftp-table"
          rowKey="path"
          size="small"
          loading={loading}
          pagination={false}
          dataSource={filteredEntries}
          rowSelection={{
            selectedRowKeys,
            onChange: setSelectedRowKeys,
            columnWidth: 44,
          }}
          onRow={(record) => ({
            onDoubleClick: () => openEntry(record),
          })}
          columns={[
            {
              title: "名称",
              dataIndex: "name",
              sorter: (left, right) => left.name.localeCompare(right.name, "zh-CN"),
              render: (value: string, record) => (
                <Button type="link" className="prototype-sftp-name" onClick={() => openEntry(record)}>
                  {record.fileType === "directory" ? <Folder size={15} /> : <Edit3 size={14} />}
                  <span>{value}</span>
                </Button>
              ),
            },
            {
              title: "大小",
              dataIndex: "size",
              width: 220,
              sorter: (left, right) => left.size - right.size,
              render: (value: number, record) => record.fileType === "directory" ? "一" : formatSftpSize(value),
            },
            {
              title: "修改时间",
              dataIndex: "modifiedAt",
              width: 420,
              sorter: (left, right) => (left.modifiedAt ?? 0) - (right.modifiedAt ?? 0),
              render: formatSftpModifiedAt,
            },
            {
              title: "操作",
              width: 260,
              align: "right",
              render: (_, record) => (
                <Space size={4} className="prototype-sftp-row-actions">
                  <Tooltip title={record.fileType === "directory" ? "打开" : "编辑"}>
                    <Button type="text" size="small" icon={<FolderOpen size={14} />} onClick={() => openEntry(record)} />
                  </Tooltip>
                  <Tooltip title="下载">
                    <Button type="text" size="small" icon={<ArrowDownToLine size={14} />} disabled={record.fileType === "directory"} onClick={() => openAction("download", record)} />
                  </Tooltip>
                  <Tooltip title="复制路径">
                    <Button type="text" size="small" icon={<Copy size={14} />} onClick={() => void copyPath(record)} />
                  </Tooltip>
                  <Tooltip title="剪切预留">
                    <Button type="text" size="small" icon={<Scissors size={14} />} onClick={() => message.info("剪切/移动会在批量操作接口接入后启用")} />
                  </Tooltip>
                  <Tooltip title="重命名">
                    <Button type="text" size="small" icon={<Pencil size={14} />} onClick={() => openAction("rename", record)} />
                  </Tooltip>
                  <Tooltip title="权限">
                    <Button type="text" size="small" icon={<KeyRound size={14} />} onClick={() => openAction("chmod", record)} />
                  </Tooltip>
                  <Tooltip title="复制链接">
                    <Button type="text" size="small" icon={<Link2 size={14} />} onClick={() => void copyPath(record)} />
                  </Tooltip>
                  <Tooltip title="删除">
                    <Button type="text" size="small" danger icon={<Trash2 size={14} />} onClick={() => openAction("delete", record)} />
                  </Tooltip>
                </Space>
              ),
            },
          ]}
        />
      </Card>
      <Drawer
        title={editorPath}
        width={920}
        open={editorOpen}
        onClose={() => setEditorOpen(false)}
        extra={<Button type="primary" loading={editorLoading} onClick={() => void saveEditor()}>保存</Button>}
      >
        <div className="prototype-sftp-editor-toolbar">
          <Space size={8}>
            <Tag color="blue">{editorLanguage.label}</Tag>
            <Text type="secondary">{editorLineCount} 行</Text>
            <Text type="secondary">{formatSftpSize(new Blob([editorContent]).size)}</Text>
          </Space>
          <Text type="secondary">支持语法高亮、行号、搜索、括号匹配、Tab 缩进和长行横向滚动</Text>
        </div>
        <div className="prototype-sftp-code-editor">
          {editorOpen ? (
            <ErrorBoundary>
              <Suspense fallback={<div className="prototype-sftp-editor-loading">编辑器加载中...</div>}>
                <SftpCodeEditor
                  value={editorContent}
                  languageKey={editorLanguage.key}
                  onChange={setEditorContent}
                />
              </Suspense>
            </ErrorBoundary>
          ) : null}
        </div>
      </Drawer>
      <Modal
        title={actionTitle[actionType]}
        open={actionOpen}
        okText={actionType === "delete" ? "确认删除" : "确认"}
        okButtonProps={{ danger: actionType === "delete" }}
        onCancel={() => setActionOpen(false)}
        onOk={() => void confirmAction()}
      >
        {actionType === "delete" ? (
          <Alert
            type="warning"
            showIcon
            message={`确认删除 ${actionEntry?.path ?? ""}？`}
            description="目录删除仅支持空目录；该操作会真实作用于远程服务器。"
          />
        ) : null}
        {actionType === "download" ? (
          <Form layout="vertical" className="mt-4">
            <Form.Item label="保存到本地路径">
              <Input value={actionValues.localPath} onChange={(event) => setActionValues((prev) => ({ ...prev, localPath: event.target.value }))} placeholder="例如 ~/Downloads/app.yml" />
            </Form.Item>
          </Form>
        ) : null}
        {actionType === "upload" ? (
          <Form layout="vertical">
            <Form.Item label="本地文件路径">
              <Input value={actionValues.localPath} onChange={(event) => setActionValues((prev) => ({ ...prev, localPath: event.target.value }))} placeholder="例如 ~/Downloads/patch.yml" />
            </Form.Item>
            <Form.Item label="远程保存路径">
              <Input value={actionValues.remotePath} onChange={(event) => setActionValues((prev) => ({ ...prev, remotePath: event.target.value }))} placeholder={joinSftpPath(currentPath, "patch.yml")} />
            </Form.Item>
          </Form>
        ) : null}
        {actionType === "uploadFolder" ? (
          <Form layout="vertical">
            <Form.Item label="本地文件夹路径">
              <Input value={actionValues.localPath} onChange={(event) => setActionValues((prev) => ({ ...prev, localPath: event.target.value }))} placeholder="例如 ~/Downloads/dist" />
            </Form.Item>
            <Form.Item label="远程保存目录">
              <Input value={actionValues.remotePath} onChange={(event) => setActionValues((prev) => ({ ...prev, remotePath: event.target.value }))} placeholder={currentPath} />
            </Form.Item>
          </Form>
        ) : null}
        {(actionType === "createFile" || actionType === "createDirectory" || actionType === "rename" || actionType === "chmod") ? (
          <Form layout="vertical">
            <Form.Item label={actionType === "rename" ? "新名称" : actionType === "chmod" ? "权限模式" : "名称"}>
              <Input value={actionValues.name} onChange={(event) => setActionValues((prev) => ({ ...prev, name: event.target.value }))} />
            </Form.Item>
          </Form>
        ) : null}
      </Modal>
    </div>
  );
}

export function EditorPage() {
  return (
    <div className="prototype-page">
      <PageHeader title="文本编辑器" description="SFTP 内置文本编辑器，保存前显示差异摘要并进入审批。" actions={<Space><Button>格式化</Button><Button type="primary">保存并申请审批</Button></Space>} />
      <TwoColumn
        left={<Card title="/opt/app/config/app.yml"><div className="prototype-editor">{["server:", "  port: 8080", "logging:", "  level: INFO", "  file: /opt/app/logs/app.log", "security:", "  require_approval: true"].map((line, index) => <div key={line} className="prototype-editor-line"><span>{index + 1}</span><code>{line}</code></div>)}</div></Card>}
        right={<><AiInsightPanel title="差异摘要"><Paragraph>将日志级别从 DEBUG 调整为 INFO，影响应用日志输出量。</Paragraph><RiskBadge level="L2" label="保存需审批" /></AiInsightPanel><Card title="编辑器设置"><Switch defaultChecked /> 自动检测换行符<br /><Switch defaultChecked /> 保存前脱敏扫描<br /><Switch /> AI 改写后自动生成说明</Card></>}
      />
    </div>
  );
}

export function ProvidersPage() {
  const [providerForm] = Form.useForm();
  const [region, setRegion] = useState<AiProviderRegion | "all">("all");
  const [providerList, setProviderList] = useState<AiProvider[]>([]);
  const [selectedProvider, setSelectedProvider] = useState<AiProvider | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [testingKey, setTestingKey] = useState<string | null>(null);
  const [loadingProviders, setLoadingProviders] = useState(false);
  const [modelOptions, setModelOptions] = useState<string[]>([]);
  const [loadingModels, setLoadingModels] = useState(false);

  function applyProviderList(items: AiProvider[]) {
    setProviderList(items);
    setSelectedProvider((current) => {
      if (current) {
        return items.find((item) => item.key === current.key) ?? items[0] ?? null;
      }
      return items[0] ?? null;
    });
  }

  async function loadAiProviders() {
    setLoadingProviders(true);
    try {
      const items = await aiProviderApi.list();
      applyProviderList(items);
    } catch (error) {
      const messageText = getErrorMessage(error);
      setProviderList([]);
      setSelectedProvider(null);
      message.error(messageText);
    } finally {
      setLoadingProviders(false);
    }
  }

  async function handleRefreshAndTestProviders() {
    setLoadingProviders(true);
    setTestingKey(ALL_PROVIDER_TEST_KEY);
    try {
      const items = await aiProviderApi.list();
      applyProviderList(items);

      const targets = items.filter(isConfiguredProvider);
      if (!targets.length) {
        message.info("Provider 列表已刷新，当前没有已启用且已配置的 Provider 需要测试");
        return;
      }

      const results = await Promise.allSettled(
        targets.map(async (provider) => ({
          provider,
          result: await aiProviderApi.test(provider.key),
        })),
      );
      const passed = results.filter((item) => item.status === "fulfilled" && item.value.result.ok).length;
      const failed = targets.length - passed;

      applyProviderList(await aiProviderApi.list());
      if (failed > 0) {
        message.warning(`已刷新并测试 ${targets.length} 个 Provider：${passed} 个成功，${failed} 个未通过`);
      } else {
        message.success(`已刷新并测试 ${targets.length} 个 Provider，全部通过`);
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setTestingKey(null);
      setLoadingProviders(false);
    }
  }

  useEffect(() => {
    void loadAiProviders();
  }, []);

  const providerStats = useMemo(() => {
    const configured = providerList.filter((item) => item.status === "configured").length;
    const reserved = providerList.filter((item) => item.status === "reserved").length;
    const china = providerList.filter((item) => item.region === "china").length;
    return [
      { label: "Provider 总数", value: String(providerList.length), hint: "来自 SQLite 真实配置" },
      { label: "已配置", value: String(configured), hint: "可用于场景路由" },
      { label: "预留适配", value: String(reserved), hint: "用户手动创建的待配置项" },
      { label: "国内模型", value: String(china), hint: "region = china" },
    ];
  }, [providerList]);

  const filteredProviders = useMemo(() => {
    if (region === "all") {
      return providerList;
    }
    return providerList.filter((item) => item.region === region);
  }, [providerList, region]);

  const providerFormInitialValues = useMemo(() => {
    if (!selectedProvider) {
      return {
        enabled: true,
        region: "china",
        protocol: "OpenAI-compatible",
        authType: "Bearer API Key",
        costLevel: "中",
      };
    }
    return selectedProvider;
  }, [selectedProvider]);

  useEffect(() => {
    if (drawerOpen) {
      providerForm.resetFields();
      providerForm.setFieldsValue(providerFormInitialValues);
      const initialModels = selectedProvider
        ? uniqueNonEmpty([selectedProvider.defaultModel, ...selectedProvider.models])
        : [];
      setModelOptions(initialModels);
    }
  }, [drawerOpen, providerForm, providerFormInitialValues]);

  async function handleProviderTest(provider: AiProvider) {
    setTestingKey(provider.key);
    try {
      const result = await aiProviderApi.test(provider.key);
      if (result.ok) {
        message.success(`${provider.name} 连接测试成功：${result.message}`);
        await loadAiProviders();
      } else {
        message.warning(`${provider.name} 连接测试未通过：${result.message}`);
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setTestingKey(null);
    }
  }

  async function handleSaveProvider(values: Record<string, unknown>) {
    const base = selectedProvider;
    const key = String(values.key ?? base?.key ?? "").trim();
    if (!key) {
      message.warning("请填写 Provider Key");
      return;
    }
    try {
      const template = providerTemplates.find((item) => item.key === key);
      const defaultModel = String(values.defaultModel ?? base?.defaultModel ?? template?.defaultModel ?? "").trim();
      const models = uniqueNonEmpty([
        defaultModel,
        ...modelOptions,
        ...(base?.models ?? []),
        ...(template?.models ?? []),
      ]);
      const input = {
        key,
        name: String(values.name ?? base?.name ?? key),
        region: (values.region ?? base?.region ?? "china") as AiProviderRegion,
        protocol: String(values.protocol ?? base?.protocol ?? "OpenAI-compatible"),
        defaultModel,
        status: getProviderSaveStatus(base, values.apiKey),
        endpoint: String(values.endpoint ?? base?.endpoint ?? ""),
        authType: String(base?.authType ?? template?.authType ?? "Bearer API Key"),
        apiKey: values.apiKey ? String(values.apiKey) : null,
        clearApiKey: false,
        costLevel: (values.costLevel ?? base?.costLevel ?? "中") as AiProvider["costLevel"],
        capabilities: base?.capabilities ?? template?.capabilities ?? [],
        models,
        scenarioFit: base?.scenarioFit ?? template?.scenarioFit ?? [],
        fallback: base?.fallback ?? template?.fallback ?? "",
        enabled: Boolean(values.enabled ?? base?.enabled ?? true),
      };
      const updated = await aiProviderApi.upsert(input);
      setSelectedProvider(updated);
      message.success("Provider 配置已保存");
      setDrawerOpen(false);
      await loadAiProviders();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  function handleProviderTemplateChange(templateKey: string) {
    const template = providerTemplates.find((item) => item.key === templateKey);
    if (!template) {
      return;
    }
    setModelOptions(uniqueNonEmpty([template.defaultModel, ...template.models]));
    providerForm.setFieldsValue(templateToFormValues(template));
  }

  async function handleLoadProviderModels() {
    const values = providerForm.getFieldsValue();
    const key = String(values.key ?? selectedProvider?.key ?? "").trim();
    const endpoint = String(values.endpoint ?? selectedProvider?.endpoint ?? "").trim();
    const protocol = String(values.protocol ?? selectedProvider?.protocol ?? "OpenAI-compatible");
    const authType = String(values.authType ?? selectedProvider?.authType ?? "Bearer API Key");
    if (!key || !endpoint) {
      message.warning("请先选择厂商模板或填写 Provider Key、Base URL");
      return;
    }
    const input: AiProviderModelListInput = {
      key,
      protocol,
      endpoint,
      authType,
      apiKey: values.apiKey ? String(values.apiKey) : null,
    };

    setLoadingModels(true);
    try {
      const result = await aiProviderApi.listModels(input);
      const nextModels = uniqueNonEmpty(result.models);
      setModelOptions(nextModels);
      const currentModel = String(providerForm.getFieldValue("defaultModel") ?? "").trim();
      if (!currentModel || !nextModels.includes(currentModel)) {
        providerForm.setFieldValue("defaultModel", nextModels[0]);
      }
      message.success(`已读取 ${nextModels.length} 个模型`);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoadingModels(false);
    }
  }

  return (
    <div className="prototype-page">
      <PageHeader
        title="AI Provider"
        description="统一管理 Anthropic、OpenAI、Gemini、DeepSeek、智谱 GLM、Kimi、MiniMax、小米 MiMo。"
        actions={<Space><Button loading={loadingProviders || testingKey === ALL_PROVIDER_TEST_KEY} onClick={() => void handleRefreshAndTestProviders()}>刷新</Button><Button type="primary" onClick={() => { setSelectedProvider(null); setModelOptions([]); setDrawerOpen(true); }}>新增 Provider</Button></Space>}
      />
      <SectionGrid columns={4}>
        {providerStats.map((item) => <StatCard key={item.label} {...item} />)}
      </SectionGrid>
      <TwoColumn
        left={
          <Card
            title="Provider 目录"
            extra={
              <Radio.Group
                size="small"
                value={region}
                onChange={(event) => setRegion(event.target.value as AiProviderRegion | "all")}
                options={(["all", "global", "china"] as Array<AiProviderRegion | "all">).map((key) => ({
                  label: providerRegionLabel[key],
                  value: key,
                }))}
                optionType="button"
              />
            }
          >
            <Table<AiProvider>
              size="small"
              rowKey="key"
              loading={loadingProviders}
              pagination={{ pageSize: 8, size: "small" }}
              dataSource={filteredProviders}
              onRow={(record) => ({
                onClick: () => setSelectedProvider(record),
              })}
              rowClassName={(record) => (selectedProvider?.key === record.key ? "prototype-selected-row" : "")}
              columns={[
                {
                  title: "Provider",
                  dataIndex: "name",
                  render: (value: string, record) => (
                    <Space orientation="vertical" size={0}>
                      <Text strong>{value}</Text>
                      <Text type="secondary">{providerRegionLabel[record.region]} · {record.protocol}</Text>
                    </Space>
                  ),
                },
                { title: "默认模型", dataIndex: "defaultModel" },
                {
                  title: "状态",
                  dataIndex: "status",
                  render: (value: AiProvider["status"]) => {
                    const meta = providerStatusMeta[value];
                    return <Badge status={meta.status} text={meta.text} />;
                  },
                },
                { title: "延迟", dataIndex: "latencyMs", render: (value: number | null) => (value ? `${value} ms` : "-") },
                {
                  title: "操作",
                  width: 136,
                  render: (_, record) => (
                    <Space size={8} style={{ width: 120 }}>
                      <Button size="small" style={{ width: 56 }} disabled={testingKey === record.key || (testingKey === ALL_PROVIDER_TEST_KEY && isConfiguredProvider(record))} onClick={(event) => { event.stopPropagation(); void handleProviderTest(record); }}>测试</Button>
                      <Button size="small" style={{ width: 56 }} onClick={(event) => { event.stopPropagation(); setSelectedProvider(record); setDrawerOpen(true); }}>配置</Button>
                    </Space>
                  ),
                },
              ]}
            />
          </Card>
        }
        right={
          <Space orientation="vertical" size={16} style={{ width: "100%" }}>
            <Card title="选中 Provider">
              {selectedProvider ? (
                <>
                  <Descriptions size="small" column={1}>
                    <Descriptions.Item label="名称">{selectedProvider.name}</Descriptions.Item>
                    <Descriptions.Item label="Endpoint">{selectedProvider.endpoint}</Descriptions.Item>
                    <Descriptions.Item label="认证">{selectedProvider.authType}</Descriptions.Item>
                    <Descriptions.Item label="密钥">{selectedProvider.hasApiKey ? selectedProvider.apiKeyMasked : "未配置"}</Descriptions.Item>
                    <Descriptions.Item label="成本等级">{selectedProvider.costLevel}</Descriptions.Item>
                    <Descriptions.Item label="最近延迟">{selectedProvider.latencyMs ? `${selectedProvider.latencyMs} ms` : "-"}</Descriptions.Item>
                  </Descriptions>
                </>
              ) : null}
            </Card>
          </Space>
        }
      />
      <Drawer
        title={selectedProvider ? `配置 ${selectedProvider.name}` : "新增 Provider"}
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        size="large"
      >
        <Form
          key={selectedProvider?.key ?? "new-provider"}
          form={providerForm}
          layout="vertical"
          initialValues={providerFormInitialValues}
          onFinish={(values) => void handleSaveProvider(values)}
        >
          {!selectedProvider ? (
            <Form.Item label="厂商模板">
              <Select
                allowClear
                placeholder="选择 Anthropic / OpenAI / Gemini / DeepSeek / GLM / Kimi / MiniMax / 小米"
                options={providerTemplates.map((item) => ({
                  label: item.name,
                  value: item.key,
                }))}
                onChange={(value) => value && handleProviderTemplateChange(value)}
              />
            </Form.Item>
          ) : null}
          <Form.Item label="Provider Key" name="key" rules={[{ required: true, message: "请填写 Provider Key" }]}>
            <Input disabled={Boolean(selectedProvider)} placeholder="例如 deepseek 或 company-gateway" />
          </Form.Item>
          <Form.Item label="Provider 名称" name="name" rules={[{ required: true, message: "请填写 Provider 名称" }]}><Input /></Form.Item>
          <Form.Item label="区域" name="region"><Select options={[{ value: "global", label: "国际" }, { value: "china", label: "国内" }]} /></Form.Item>
          <Form.Item label="协议适配器" name="protocol"><Select options={[{ value: "OpenAI-compatible" }, { value: "OpenAI-compatible / Anthropic-compatible" }, { value: "OpenAI Responses / Chat Completions" }, { value: "Messages API" }, { value: "Gemini API" }, { value: "MiniMax API" }]} /></Form.Item>
          <Form.Item label="Base URL" name="endpoint" rules={[{ required: true, message: "请填写 Base URL" }]}><Input /></Form.Item>
          <Form.Item label="认证方式" name="authType"><Input disabled /></Form.Item>
          <Form.Item label="默认模型" required>
            <Space.Compact style={{ width: "100%" }}>
              <Form.Item name="defaultModel" noStyle rules={[{ required: true, message: "请选择默认模型" }]}>
                <Select
                  showSearch
                  loading={loadingModels}
                  placeholder="先选择厂商模板或点击读取模型列表"
                  options={modelOptions.map((model) => ({ label: model, value: model }))}
                  filterOption={(input, option) =>
                    String(option?.label ?? "").toLowerCase().includes(input.toLowerCase())
                  }
                />
              </Form.Item>
              <Button loading={loadingModels} onClick={() => void handleLoadProviderModels()}>
                读取模型列表
              </Button>
            </Space.Compact>
          </Form.Item>
          <Form.Item label="系统状态">
            <Tag color={selectedProvider ? providerStatusMeta[selectedProvider.status].color : providerStatusMeta.unconfigured.color}>
              {selectedProvider ? providerStatusMeta[selectedProvider.status].text : providerStatusMeta.unconfigured.text}
            </Tag>
          </Form.Item>
          <Form.Item label="成本等级" name="costLevel"><Select options={[{ value: "低" }, { value: "中" }, { value: "高" }, { value: "企业" }]} /></Form.Item>
          <Form.Item label="启用" name="enabled" valuePropName="checked"><Switch /></Form.Item>
          <Form.Item label="API Key" name="apiKey">
            <Input.Password placeholder="留空则保留现有密钥" />
          </Form.Item>
          <Alert type="warning" showIcon title="API Key 只发送给 Rust 后端加密存储；列表接口只返回掩码和 hasApiKey。" />
          <Divider />
          <Space>
            <Button onClick={() => selectedProvider && void handleProviderTest(selectedProvider)} loading={testingKey === selectedProvider?.key}>测试连接</Button>
            <Button type="primary" htmlType="submit">保存配置</Button>
          </Space>
        </Form>
      </Drawer>
    </div>
  );
}

export function McpPage() {
  const [mcpOverview, setMcpOverview] = useState<McpOverview | null>(null);
  const [loadingMcp, setLoadingMcp] = useState(false);
  const [configuringClientKey, setConfiguringClientKey] = useState<string | null>(null);

  const statusMeta: Record<string, { text: string; color: string; badge: "success" | "default" | "warning" }> = {
    configured: { text: "已接入", color: "green", badge: "success" },
    available: { text: "可接入", color: "blue", badge: "warning" },
    not_found: { text: "未发现配置", color: "default", badge: "default" },
  };

  const loadMcpOverview = useCallback(async () => {
    setLoadingMcp(true);
    try {
      setMcpOverview(await mcpApi.overview());
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoadingMcp(false);
    }
  }, []);

  useEffect(() => {
    void loadMcpOverview();
  }, [loadMcpOverview]);

  async function copyMcpText(content: string) {
    try {
      await navigator.clipboard.writeText(content);
      message.success("已复制");
    } catch {
      message.error("复制失败，请手动选择文本复制");
    }
  }

  async function configureClient(client: McpClientConfig) {
    setConfiguringClientKey(client.key);
    try {
      const result = await mcpApi.configureClient({ clientKey: client.key, transport: client.transport });
      message.success(result.backupPath ? `${result.message}，原配置已备份` : result.message);
      await loadMcpOverview();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setConfiguringClientKey(null);
    }
  }

  const status = mcpOverview?.status;
  const stdioSnippet = status
    ? JSON.stringify({ mcpServers: { [status.serverName]: { command: status.stdioCommand, args: status.stdioArgs } } }, null, 2)
    : "";

  return (
    <div className="prototype-page prototype-mcp-page">
      <PageHeader
        title="MCP 接入中心"
        description="把 Tauri SSH 暴露的 MCP 服务接到 Claude Code / Codex / Cursor / Cline / Zed / OpenCode 等 Agent 工具。"
        actions={
          <Space>
            {status ? (
              <Tag color={!status.enabled ? "default" : status.httpReachable ? "green" : "orange"}>
                {!status.enabled ? "MCP 已关闭" : status.httpReachable ? "HTTP 可用" : "等待本地端点"}
              </Tag>
            ) : null}
            <Button icon={<RefreshCw size={15} />} loading={loadingMcp} onClick={() => void loadMcpOverview()}>刷新</Button>
          </Space>
        }
      />

      <Card
        className="prototype-mcp-card"
        title={<span className="prototype-card-title"><PlugZap size={16} />端点状态（仅本地 127.0.0.1）</span>}
        extra={<Button onClick={() => status && void copyMcpText(status.streamableHttpUrl)}>复制端点</Button>}
        loading={loadingMcp && !mcpOverview}
      >
        {status && !status.enabled ? (
          <Alert
            type="warning"
            showIcon
            message="MCP Server 已关闭"
            description="可在系统设置中开启。开发环境默认关闭，release 版本默认开启。"
            style={{ marginBottom: 16 }}
          />
        ) : null}
        {status ? (
          <div className="prototype-mcp-endpoint-grid">
            <div>
              <Text type="secondary">Streamable HTTP 端点</Text>
              <CodeBlock>{status.streamableHttpUrl}</CodeBlock>
            </div>
            <div>
              <Text type="secondary">stdio bridge 命令</Text>
              <CodeBlock>{[status.stdioCommand, ...status.stdioArgs].join(" ")}</CodeBlock>
            </div>
            <div className="prototype-mcp-note">
              {status.notes.map((note) => <Paragraph key={note}>{note}</Paragraph>)}
            </div>
          </div>
        ) : (
          <Alert type="info" showIcon message="正在读取 MCP 状态" />
        )}
      </Card>

      <Card
        className="prototype-mcp-card"
        title={<span className="prototype-card-title"><Link2 size={16} />一键接入 AI 客户端</span>}
        extra={<Text type="secondary">点击后写入 scoped 配置，重启对应客户端生效</Text>}
      >
        <div className="prototype-mcp-client-list">
          {(mcpOverview?.clients ?? []).map((client) => {
            const meta = statusMeta[client.status] ?? statusMeta.not_found;
            return (
              <div className="prototype-mcp-client-row" key={client.key}>
                <div className="prototype-mcp-client-icon">
                  <Bot size={18} />
                </div>
                <div className="prototype-mcp-client-main">
                  <Space size={8}>
                    <Text strong>{client.name}</Text>
                    {client.configured ? <Tag color="green">已接入</Tag> : <Tag color={meta.color}>{meta.text}</Tag>}
                  </Space>
                  <div className="prototype-mcp-client-desc">{client.description}</div>
                  <Text type="secondary">{client.configPath}</Text>
                </div>
                <Button
                  type={client.configured ? "default" : "primary"}
                  loading={configuringClientKey === client.key}
                  onClick={() => void configureClient(client)}
                >
                  {client.configured ? "重新接入" : "一键接入"}
                </Button>
              </div>
            );
          })}
        </div>
      </Card>

      <SectionGrid columns={2}>
        <Card title={<span className="prototype-card-title"><KeyRound size={16} />工具权限</span>} loading={loadingMcp && !mcpOverview}>
          <Table
            size="small"
            pagination={{ pageSize: 10, showSizeChanger: false, size: "small" }}
            rowKey="tool"
            dataSource={mcpOverview?.tools ?? []}
            columns={[
              { title: "工具", dataIndex: "tool" },
              { title: "策略", dataIndex: "policy" },
              { title: "审计", dataIndex: "audit" },
            ]}
          />
        </Card>
        <Card title={<span className="prototype-card-title"><Copy size={16} />手动接入</span>}>
          <Tabs
            items={(mcpOverview?.snippets ?? [
              { title: "stdio 接入", transport: "stdio", content: stdioSnippet },
            ]).map((snippet) => ({
              key: snippet.title,
              label: snippet.title,
              children: (
                <>
                  <div className="prototype-mcp-snippet-actions">
                    <Tag>{snippet.transport}</Tag>
                    <Button size="small" icon={<Copy size={14} />} onClick={() => void copyMcpText(snippet.content)}>复制</Button>
                  </div>
                  <CodeBlock>{snippet.content}</CodeBlock>
                </>
              ),
            }))}
          />
        </Card>
      </SectionGrid>
    </div>
  );
}

export function JumpServerPage() {
  const [sessions, setSessions] = useState<JumpServerSession[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editing, setEditing] = useState<JumpServerSession | null>(null);
  const [form] = Form.useForm<UpsertJumpServerSessionInput>();

  const loadSessions = useCallback(async () => {
    setLoading(true);
    try {
      setSessions(await jumpserverApi.list());
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadSessions();
  }, [loadSessions]);

  const openCreateDrawer = () => {
    setEditing(null);
    form.setFieldsValue({
      key: `jumpserver-${Date.now()}`,
      name: "",
      endpoint: "",
      webUrl: "",
      sessionRef: "",
      groupName: "堡垒机",
      accountHint: "",
      assetHint: "",
      protocol: "web_ssh",
      aiMode: "suggest_only",
      status: "available",
      notes: "",
      enabled: true,
    });
    setDrawerOpen(true);
  };

  const openEditDrawer = (record: JumpServerSession) => {
    setEditing(record);
    form.setFieldsValue({
      key: record.key,
      name: record.name,
      endpoint: record.endpoint,
      webUrl: record.webUrl,
      sessionRef: record.sessionRef,
      groupName: record.groupName,
      accountHint: record.accountHint,
      assetHint: record.assetHint,
      protocol: record.protocol,
      aiMode: record.aiMode,
      status: record.status,
      notes: record.notes,
      enabled: record.enabled,
    });
    setDrawerOpen(true);
  };

  const handleSave = async () => {
    try {
      const values = await form.validateFields();
      setSaving(true);
      await jumpserverApi.upsert(values);
      message.success(editing ? "堡垒机会话已更新" : "堡垒机会话已新增");
      setDrawerOpen(false);
      await loadSessions();
    } catch (error) {
      if (typeof error === "object" && error !== null && "errorFields" in error) return;
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const handleOpen = async (record: JumpServerSession) => {
    try {
      const result = await jumpserverApi.open(record.key);
      message.success(result.message);
      await loadSessions();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const handleDelete = async (record: JumpServerSession) => {
    try {
      await jumpserverApi.delete(record.key);
      message.success("堡垒机会话已删除");
      await loadSessions();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const statusMap: Record<JumpServerStatus, { text: string; color: string }> = {
    unknown: { text: "未知", color: "default" },
    available: { text: "可用", color: "green" },
    opened: { text: "已打开", color: "blue" },
    expired: { text: "已过期", color: "orange" },
    disabled: { text: "已禁用", color: "red" },
  };
  const protocolMap: Record<JumpServerProtocol, string> = {
    web_ssh: "Web SSH",
    web_sftp: "Web SFTP",
    jumpserver_asset: "资产入口",
  };
  const aiModeMap: Record<JumpServerAiMode, string> = {
    suggest_only: "仅建议",
    disabled: "禁用 AI",
  };
  const enabledCount = sessions.filter((item) => item.enabled).length;
  const openedCount = sessions.filter((item) => item.status === "opened").length;

  const columns: TableProps<JumpServerSession>["columns"] = [
    {
      title: "名称",
      dataIndex: "name",
      width: 180,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Text strong>{record.name}</Text>
          <Text type="secondary">{record.key}</Text>
        </Space>
      ),
    },
    {
      title: "入口地址",
      dataIndex: "endpoint",
      ellipsis: true,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Text>{record.endpoint}</Text>
          <Text type="secondary" copyable>{record.webUrl}</Text>
        </Space>
      ),
    },
    { title: "分组", dataIndex: "groupName", width: 120 },
    { title: "资产提示", dataIndex: "assetHint", width: 140, render: (value) => value || "-" },
    { title: "账号提示", dataIndex: "accountHint", width: 120, render: (value) => value || "-" },
    {
      title: "协议",
      dataIndex: "protocol",
      width: 110,
      render: (value: JumpServerProtocol) => protocolMap[value] ?? value,
    },
    {
      title: "AI 模式",
      dataIndex: "aiMode",
      width: 100,
      render: (value: JumpServerAiMode) => aiModeMap[value] ?? value,
    },
    {
      title: "状态",
      dataIndex: "status",
      width: 90,
      render: (value: JumpServerStatus, record) => {
        const status = record.enabled ? value : "disabled";
        const meta = statusMap[status];
        return <Tag color={meta.color}>{meta.text}</Tag>;
      },
    },
    {
      title: "最近打开",
      dataIndex: "lastOpenedAt",
      width: 160,
      render: (value) => value || "-",
    },
    {
      title: "操作",
      key: "actions",
      width: 220,
      fixed: "right",
      render: (_, record) => (
        <Space>
          <Button size="small" type="primary" disabled={!record.enabled} onClick={() => void handleOpen(record)}>
            打开
          </Button>
          <Button size="small" onClick={() => openEditDrawer(record)}>编辑</Button>
          <Popconfirm title="确认删除该堡垒机会话入口？" onConfirm={() => void handleDelete(record)}>
            <Button size="small" danger>删除</Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <div className="prototype-page">
      <PageHeader
        title="堡垒机会话"
        description="管理 ISC / JumpServer Web SSH 合规入口，只保存会话引用和入口地址，不提取浏览器凭据。"
        actions={<Space><Button onClick={() => void loadSessions()} loading={loading}>刷新</Button><Button type="primary" onClick={openCreateDrawer}>新增会话入口</Button></Space>}
      />
      <SectionGrid columns={3}>
        <StatCard label="会话入口" value={String(sessions.length)} hint={`启用 ${enabledCount} 个`} />
        <StatCard label="最近打开" value={String(openedCount)} hint="打开入口会记录时间和状态" />
        <Card title="安全边界">
          <Space direction="vertical">
            <Tag color="red">合规约束</Tag>
            <Paragraph className="!mb-0">不读取 ISC Cookie、不提取 JumpServer 凭据、不绕过堡垒机权限；AI 仅提供命令建议和输出解释。</Paragraph>
          </Space>
        </Card>
      </SectionGrid>
      <Card title="会话入口目录">
        <Table
          rowKey="key"
          columns={columns}
          dataSource={sessions}
          loading={loading}
          scroll={{ x: 1320 }}
          pagination={{ pageSize: 10, showSizeChanger: true }}
        />
      </Card>
      <Drawer
        title={editing ? "编辑堡垒机会话入口" : "新增堡垒机会话入口"}
        width={620}
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        extra={<Space><Button onClick={() => setDrawerOpen(false)}>取消</Button><Button type="primary" loading={saving} onClick={() => void handleSave()}>保存</Button></Space>}
      >
        <Alert
          className="mb-4"
          type="warning"
          showIcon
          message="只保存合规入口"
          description="这里保存的是 JumpServer Web SSH / Web SFTP 页面入口和备注，不保存密码、私钥、Cookie 或 ISC 登录态。"
        />
        <Form form={form} layout="vertical">
          <Form.Item name="key" label="会话 Key" rules={[{ required: true, message: "请输入会话 Key" }]}>
            <Input disabled={Boolean(editing)} placeholder="jumpserver-prod-webssh" />
          </Form.Item>
          <Form.Item name="name" label="名称" rules={[{ required: true, message: "请输入名称" }]}>
            <Input placeholder="生产堡垒机 Web SSH" />
          </Form.Item>
          <Form.Item name="endpoint" label="堡垒机入口" rules={[{ required: true, message: "请输入堡垒机入口" }]}>
            <Input placeholder="https://jumpserver.example.com" />
          </Form.Item>
          <Form.Item name="webUrl" label="Web SSH URL" rules={[{ required: true, message: "请输入 Web SSH URL" }, { type: "url", message: "请输入合法 URL" }]}>
            <Input placeholder="https://jumpserver.example.com/luna/..." />
          </Form.Item>
          <Form.Item name="sessionRef" label="会话引用">
            <Input placeholder="可填写 JumpServer 资产 ID、页面标题或内部单号" />
          </Form.Item>
          <Form.Item name="groupName" label="分组" rules={[{ required: true, message: "请输入分组" }]}>
            <Input placeholder="堡垒机" />
          </Form.Item>
          <Form.Item name="assetHint" label="资产提示">
            <Input placeholder="例如 bailing-dev-71 / 生产数据库只读机" />
          </Form.Item>
          <Form.Item name="accountHint" label="账号提示">
            <Input placeholder="例如 ops_user，不能填写密码" />
          </Form.Item>
          <Form.Item name="protocol" label="协议" rules={[{ required: true }]}>
            <Select
              options={[
                { label: "Web SSH", value: "web_ssh" },
                { label: "Web SFTP", value: "web_sftp" },
                { label: "资产入口", value: "jumpserver_asset" },
              ]}
            />
          </Form.Item>
          <Form.Item name="aiMode" label="AI 模式" rules={[{ required: true }]}>
            <Select
              options={[
                { label: "仅建议，不自动输入 Web SSH", value: "suggest_only" },
                { label: "禁用 AI", value: "disabled" },
              ]}
            />
          </Form.Item>
          <Form.Item name="status" label="状态">
            <Select
              options={[
                { label: "未知", value: "unknown" },
                { label: "可用", value: "available" },
                { label: "已打开", value: "opened" },
                { label: "已过期", value: "expired" },
                { label: "已禁用", value: "disabled" },
              ]}
            />
          </Form.Item>
          <Form.Item name="enabled" label="启用" valuePropName="checked">
            <Switch checkedChildren="启用" unCheckedChildren="禁用" />
          </Form.Item>
          <Form.Item name="notes" label="备注">
            <Input.TextArea rows={4} placeholder="记录申请来源、可访问资产范围、注意事项等" />
          </Form.Item>
        </Form>
      </Drawer>
    </div>
  );
}

export function AuditPage() {
  const [logs, setLogs] = useState<AuditLog[]>([]);
  const [loading, setLoading] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [selectedLog, setSelectedLog] = useState<AuditLog | null>(null);
  const [filterForm] = Form.useForm<ListAuditLogsInput>();

  const currentFilters = useCallback((): ListAuditLogsInput => {
    const values = filterForm.getFieldsValue();
    return {
      actor: values.actor || null,
      source: values.source || null,
      serverAlias: values.serverAlias || null,
      action: values.action || null,
      risk: values.risk || null,
      result: values.result || null,
      keyword: values.keyword || null,
      limit: values.limit || 200,
    };
  }, [filterForm]);

  const loadLogs = useCallback(async () => {
    setLoading(true);
    try {
      setLogs(await auditApi.list(currentFilters()));
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [currentFilters]);

  useEffect(() => {
    filterForm.setFieldsValue({ limit: 200 });
    void loadLogs();
  }, [filterForm, loadLogs]);

  const handleReset = () => {
    filterForm.resetFields();
    filterForm.setFieldsValue({ limit: 200 });
    void loadLogs();
  };

  const handleExport = async () => {
    setExporting(true);
    try {
      const result = await auditApi.export(currentFilters());
      const blob = new Blob([result.content], { type: "application/json;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = result.fileName;
      link.click();
      URL.revokeObjectURL(url);
      message.success(`已导出 ${result.count} 条审计日志`);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setExporting(false);
    }
  };

  const riskCount = logs.filter((item) => ["L2", "L3", "blocked"].includes(item.risk)).length;
  const serverCount = new Set(logs.map((item) => item.serverAlias).filter(Boolean)).size;
  const actorCount = new Set(logs.map((item) => item.actor).filter(Boolean)).size;
  const auditLogColumns: TableProps<AuditLog>["columns"] = [
    { title: "时间", dataIndex: "occurredAt", width: 160 },
    { title: "操作者", dataIndex: "actor", width: 150, ellipsis: true },
    { title: "来源", dataIndex: "source", width: 130 },
    { title: "服务器", dataIndex: "serverAlias", width: 150, render: (value) => value || "-" },
    { title: "动作", dataIndex: "action", width: 150 },
    { title: "风险", dataIndex: "risk", width: 100, render: (value: AuditRisk) => <RiskBadge level={value} /> },
    { title: "结果", dataIndex: "result", width: 120 },
    { title: "摘要", dataIndex: "summary", ellipsis: true },
    {
      title: "操作",
      width: 90,
      fixed: "right",
      render: (_, record) => <Button size="small" onClick={() => setSelectedLog(record)}>详情</Button>,
    },
  ];

  return (
    <div className="prototype-page">
      <PageHeader
        title="审计日志"
        description="命令、日志监听、搜索过滤、SFTP、AI、MCP、审批链路统一审计并脱敏。"
        actions={<Space><Button onClick={() => void loadLogs()} loading={loading}>刷新</Button><Button onClick={() => void handleExport()} loading={exporting}>导出脱敏日志</Button></Space>}
      />
      <SectionGrid columns={4}>
        <StatCard label="日志条数" value={String(logs.length)} hint="当前筛选结果" />
        <StatCard label="风险事件" value={String(riskCount)} hint="L2 / L3 / blocked" />
        <StatCard label="涉及服务器" value={String(serverCount)} hint="按服务器别名去重" />
        <StatCard label="操作者" value={String(actorCount)} hint="按 actor 去重" />
      </SectionGrid>
      <Card title="筛选条件" className="prototype-audit-filter-card">
        <Form form={filterForm} layout="vertical" onFinish={() => void loadLogs()} className="prototype-audit-filter-form">
          <Form.Item name="keyword" label="关键词"><Input allowClear placeholder="摘要 / 动作 / 结果" /></Form.Item>
          <Form.Item name="actor" label="操作者"><Input allowClear placeholder="user / ai / mcp" /></Form.Item>
          <Form.Item name="source" label="来源"><Input allowClear placeholder="terminal / sftp / mcp" /></Form.Item>
          <Form.Item name="serverAlias" label="服务器"><Input allowClear placeholder="服务器别名" /></Form.Item>
          <Form.Item name="risk" label="风险">
            <Select
              allowClear
              options={["L0", "L1", "L2", "L3", "readonly", "blocked", "ai"].map((value) => ({ label: value, value }))}
            />
          </Form.Item>
          <Form.Item name="result" label="结果"><Input allowClear placeholder="成功 / 失败 / 拒绝" /></Form.Item>
          <Form.Item name="limit" label="数量">
            <Select
              options={[100, 200, 500, 1000, 5000].map((value) => ({ label: String(value), value }))}
            />
          </Form.Item>
          <Form.Item label=" " className="prototype-audit-filter-actions">
            <Space><Button htmlType="submit" type="primary">筛选</Button><Button onClick={handleReset}>重置</Button></Space>
          </Form.Item>
        </Form>
      </Card>
      <Card title="审计明细">
        <Table
          rowKey="id"
          columns={auditLogColumns}
          dataSource={logs}
          loading={loading}
          scroll={{ x: 1250 }}
          pagination={{ pageSize: 20, showSizeChanger: true }}
        />
      </Card>
      <Modal
        title="审计详情"
        open={Boolean(selectedLog)}
        onCancel={() => setSelectedLog(null)}
        footer={<Button onClick={() => setSelectedLog(null)}>关闭</Button>}
        width={760}
      >
        {selectedLog && (
          <Space direction="vertical" className="w-full">
            <Descriptions bordered size="small" column={2}>
              <Descriptions.Item label="ID">{selectedLog.id}</Descriptions.Item>
              <Descriptions.Item label="时间">{selectedLog.occurredAt}</Descriptions.Item>
              <Descriptions.Item label="操作者">{selectedLog.actor}</Descriptions.Item>
              <Descriptions.Item label="来源">{selectedLog.source}</Descriptions.Item>
              <Descriptions.Item label="服务器">{selectedLog.serverAlias || "-"}</Descriptions.Item>
              <Descriptions.Item label="动作">{selectedLog.action}</Descriptions.Item>
              <Descriptions.Item label="风险"><RiskBadge level={selectedLog.risk} /></Descriptions.Item>
              <Descriptions.Item label="结果">{selectedLog.result}</Descriptions.Item>
              <Descriptions.Item label="请求 ID">{selectedLog.requestId || "-"}</Descriptions.Item>
              <Descriptions.Item label="审批 ID">{selectedLog.approvalId ?? "-"}</Descriptions.Item>
              <Descriptions.Item label="摘要" span={2}>{selectedLog.summary}</Descriptions.Item>
            </Descriptions>
            <CodeBlock>{selectedLog.detailJson || "{}"}</CodeBlock>
          </Space>
        )}
      </Modal>
    </div>
  );
}

export function WorkspacePage() {
  return (
    <div className="prototype-page">
      <PageHeader title="团队预留" description="V0.1 先预留 workspace、user、role、server scope 字段，默认本地个人空间。" />
      <SectionGrid columns={3}>
        <Card title="Workspace"><Title level={3}>local-personal</Title><Text type="secondary">默认本机个人空间</Text></Card>
        <Card title="Roles"><Tag>Owner</Tag><Tag>Operator</Tag><Tag>Auditor</Tag></Card>
        <Card title="Server Scope"><Paragraph>服务器、凭据、审批策略、审计日志均带 workspace_id 预留字段。</Paragraph></Card>
      </SectionGrid>
    </div>
  );
}

export function PrototypeSettingsPage() {
  const [form] = Form.useForm<SystemSettings>();
  const [settings, setSettings] = useState<SystemSettings | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [dangerousRuleModalOpen, setDangerousRuleModalOpen] = useState(false);
  const [dangerousRuleInput, setDangerousRuleInput] = useState("");
  const setTheme = useAppStore((state) => state.setTheme);
  const watchedDangerousCommands = Form.useWatch("dangerousCommands", form);

  const applySettings = useCallback((next: SystemSettings) => {
    setSettings(next);
    form.setFieldsValue(next);
    setTheme(next.theme);
  }, [form, setTheme]);

  const loadSettings = useCallback(async () => {
    setLoading(true);
    try {
      applySettings(await systemSettingsApi.get());
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [applySettings]);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  const saveSettings = async () => {
    try {
      const values = await form.validateFields();
      setSaving(true);
      const next = await systemSettingsApi.update(values);
      applySettings(next);
      message.success("系统设置已保存");
    } catch (error) {
      if (typeof error === "object" && error !== null && "errorFields" in error) return;
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const resetSettings = () => {
    Modal.confirm({
      title: "恢复默认设置？",
      content: "将重置主题、更新、审计保留、日志级别、危险命令黑名单和关闭行为等设置。",
      okText: "恢复默认",
      cancelText: "取消",
      async onOk() {
        const next = await systemSettingsApi.reset();
        applySettings(next);
        message.success("已恢复默认设置");
      },
    });
  };

  const exportSettings = async () => {
    setExporting(true);
    try {
      const result = await systemSettingsApi.export();
      const blob = new Blob([result.content], { type: "application/json;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = result.fileName;
      link.click();
      URL.revokeObjectURL(url);
      message.success("系统设置已导出");
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setExporting(false);
    }
  };

  const dangerousCommands = normalizeDangerousCommandList(
    Array.isArray(watchedDangerousCommands)
      ? watchedDangerousCommands
      : settings?.dangerousCommands ?? dangerousCommandPresets.map((item) => item.pattern),
  );
  const dangerousRows: DangerousCommandTableRow[] = dangerousCommands.map((pattern) => {
    const description = dangerousPresetMap.get(pattern);
    return {
      key: pattern,
      pattern,
      description: description ?? "用户自定义规则",
      source: description ? "builtin" : "user",
    };
  });
  const builtinDangerousCount = dangerousRows.filter((item) => item.source === "builtin").length;
  const userDangerousCount = dangerousRows.length - builtinDangerousCount;

  const setDangerousCommands = (commands: string[]) => {
    form.setFieldValue("dangerousCommands", normalizeDangerousCommandList(commands));
  };

  const addDangerousRule = () => {
    const value = dangerousRuleInput.trim();
    if (!value) {
      message.warning("请输入危险命令正则");
      return;
    }
    const next = normalizeDangerousCommandList([...dangerousCommands, value]);
    if (next.length === dangerousCommands.length) {
      message.info("该规则已存在");
      return;
    }
    setDangerousCommands(next);
    setDangerousRuleInput("");
    setDangerousRuleModalOpen(false);
  };

  const removeDangerousRule = (pattern: string) => {
    if (dangerousPresetMap.has(pattern)) {
      message.warning("内置危险命令规则不可删除");
      return;
    }
    setDangerousCommands(dangerousCommands.filter((item) => item !== pattern));
  };

  const dangerousColumns: TableProps<DangerousCommandTableRow>["columns"] = [
    {
      title: "正则",
      dataIndex: "pattern",
      render: (value: string) => <Text code className="prototype-dangerous-command-pattern">{value}</Text>,
    },
    {
      title: "说明",
      dataIndex: "description",
      width: 360,
      render: (value: string) => <Text strong>{value}</Text>,
    },
    {
      title: "来源",
      dataIndex: "source",
      width: 130,
      render: (value: DangerousCommandTableRow["source"]) => (
        <Tag color={value === "builtin" ? "default" : "blue"}>{value === "builtin" ? "内置" : "用户"}</Tag>
      ),
    },
    {
      title: "操作",
      width: 96,
      align: "center",
      render: (_, row) => (
        <Tooltip title={row.source === "builtin" ? "内置规则不可删除" : "删除规则"}>
          <Button
            type="text"
            danger={row.source === "user"}
            disabled={row.source === "builtin"}
            icon={<Trash2 size={16} />}
            onClick={() => removeDangerousRule(row.pattern)}
          />
        </Tooltip>
      ),
    },
  ];

  return (
    <div className="prototype-page">
      <PageHeader
        title="系统设置"
        description="主题、更新、日志、数据备份、保留期和跨平台行为设置。"
        actions={<Space><Button onClick={() => void loadSettings()} loading={loading}>刷新</Button><Button onClick={() => void exportSettings()} loading={exporting}>导出设置</Button></Space>}
      />
      <Card
        title="基础设置"
        loading={loading && !settings}
        extra={<Space><Button onClick={resetSettings}>恢复默认</Button><Button type="primary" loading={saving} onClick={() => void saveSettings()}>保存设置</Button></Space>}
      >
        <Form<SystemSettings> form={form} layout="vertical" disabled={loading || saving}>
          <SectionGrid columns={3}>
            <Form.Item label="主题" name="theme" rules={[{ required: true, message: "请选择主题" }]}>
              <Select options={[{ value: "system", label: "跟随系统" }, { value: "light", label: "浅色" }, { value: "dark", label: "深色" }]} />
            </Form.Item>
            <Form.Item label="自动更新" name="autoUpdate" valuePropName="checked">
              <Switch checkedChildren="开启" unCheckedChildren="关闭" />
            </Form.Item>
            <Form.Item
              label="MCP Server"
              name="mcpEnabled"
              valuePropName="checked"
              tooltip="控制本地 MCP endpoint 是否允许 Agent 调用；开发版本默认关闭，release 版本默认开启。"
            >
              <Switch checkedChildren="开启" unCheckedChildren="关闭" />
            </Form.Item>
            <Form.Item
              label="随系统启动"
              name="launchOnStartup"
              valuePropName="checked"
              tooltip="支持 macOS 和 Windows；开启后应用会在系统登录后自动启动。"
            >
              <Switch checkedChildren="开启" unCheckedChildren="关闭" />
            </Form.Item>
            <Form.Item label="审计保留天数" name="auditRetentionDays" rules={[{ required: true, message: "请输入审计保留天数" }]}>
              <InputNumber className="w-full" min={1} max={3650} addonAfter="天" />
            </Form.Item>
            <Form.Item label="日志级别" name="logLevel" rules={[{ required: true, message: "请选择日志级别" }]}>
              <Select options={[
                { value: "debug", label: "Debug" },
                { value: "info", label: "Info" },
                { value: "warn", label: "Warn" },
                { value: "error", label: "Error" },
              ]} />
            </Form.Item>
            <Form.Item label="备份位置" name="backupDir" rules={[{ required: true, message: "请输入备份位置" }]}>
              <Input placeholder="应用数据目录 / backups" />
            </Form.Item>
            <Form.Item label="数据库导出目录" name="databaseDownloadDir" rules={[{ required: true, message: "请输入数据库导出目录" }]}>
              <Input placeholder="~/Downloads" />
            </Form.Item>
            <Form.Item label="首发平台" name="platform" rules={[{ required: true }]}>
              <Select options={[{ value: "macos-windows", label: "macOS + Windows" }]} />
            </Form.Item>
            <Form.Item label="关闭行为" name="closeBehavior" rules={[{ required: true }]}>
              <Select options={[{ value: "minimize", label: "关闭到托盘" }, { value: "exit", label: "直接退出" }]} />
            </Form.Item>
            <Form.Item label="语言" name="language" rules={[{ required: true }]}>
              <Select options={[{ value: "zh-CN", label: "简体中文" }, { value: "en-US", label: "English" }]} />
            </Form.Item>
          </SectionGrid>
          <Form.Item
            noStyle
            name="dangerousCommands"
            rules={[{ required: true, message: "请至少保留一条危险命令规则" }]}
          >
            <DangerousCommandsField />
          </Form.Item>
        </Form>
      </Card>
      <Card
        className="prototype-dangerous-command-card"
        title={(
          <div className="prototype-dangerous-command-title">
            <ShieldAlert size={18} />
            <span>危险命令黑名单</span>
          </div>
        )}
        extra={(
          <Button
            type="primary"
            icon={<Plus size={16} />}
            onClick={() => setDangerousRuleModalOpen(true)}
          >
            新增
          </Button>
        )}
      >
        <div className="prototype-dangerous-command-desc">
          正则匹配，命中即 <Text code>blocked</Text>（所有策略档位都生效，含 <Text code>trusted</Text>）。
          内置 <Text strong type="success">{builtinDangerousCount}</Text> 条 + 用户自加 <Text strong>{userDangerousCount}</Text> 条。
        </div>
        <Table<DangerousCommandTableRow>
          className="prototype-dangerous-command-table"
          rowKey="key"
          columns={dangerousColumns}
          dataSource={dangerousRows}
          pagination={{ pageSize: 10, showSizeChanger: false }}
          scroll={{ x: 980 }}
        />
      </Card>
      <Modal
        title="新增危险命令规则"
        open={dangerousRuleModalOpen}
        okText="新增"
        cancelText="取消"
        onOk={addDangerousRule}
        onCancel={() => {
          setDangerousRuleModalOpen(false);
          setDangerousRuleInput("");
        }}
      >
        <Space direction="vertical" className="w-full">
          <Text type="secondary">请输入正则表达式。保存设置后，该规则会参与本地命令阻止。</Text>
          <Input.TextArea
            autoSize={{ minRows: 3, maxRows: 6 }}
            value={dangerousRuleInput}
            onChange={(event) => setDangerousRuleInput(event.target.value)}
            placeholder={String.raw`\brm\s+-rf\s+/tmp/important`}
          />
        </Space>
      </Modal>
      <Card title="当前状态">
        <Descriptions column={3} size="small" bordered>
          <Descriptions.Item label="主题">{settings?.theme ?? "-"}</Descriptions.Item>
          <Descriptions.Item label="自动更新">{settings?.autoUpdate ? "开启" : "关闭"}</Descriptions.Item>
          <Descriptions.Item label="MCP Server">{settings?.mcpEnabled ? "开启" : "关闭"}</Descriptions.Item>
          <Descriptions.Item label="随系统启动">{settings?.launchOnStartup ? "开启" : "关闭"}</Descriptions.Item>
          <Descriptions.Item label="审计保留">{settings ? `${settings.auditRetentionDays} 天` : "-"}</Descriptions.Item>
          <Descriptions.Item label="日志级别">{settings?.logLevel ?? "-"}</Descriptions.Item>
          <Descriptions.Item label="备份位置">{settings?.backupDir ?? "-"}</Descriptions.Item>
          <Descriptions.Item label="数据库导出目录">{settings?.databaseDownloadDir ?? "-"}</Descriptions.Item>
          <Descriptions.Item label="关闭行为">{settings?.closeBehavior === "exit" ? "直接退出" : "关闭到托盘"}</Descriptions.Item>
          <Descriptions.Item label="AI 临时放行">
            {settings?.aiUnrestrictedUntil ? `截至 ${new Date(settings.aiUnrestrictedUntil).toLocaleString()}` : "关闭"}
          </Descriptions.Item>
          <Descriptions.Item label="危险命令规则">{settings?.dangerousCommands.length ?? 0} 条</Descriptions.Item>
        </Descriptions>
      </Card>
    </div>
  );
}

export function StatesPage() {
  return (
    <div className="prototype-page">
      <PageHeader title="状态页" description="覆盖空状态、错误状态、权限不足、连接失败、审批等待和重连状态。" />
      <SectionGrid columns={3}>
        <Alert type="info" showIcon title="暂无服务器" description="从 SSH Config 导入或手工新增服务器。" />
        <Alert type="error" showIcon title="连接失败" description="SSH 握手超时，建议检查网络、端口和跳板机。" />
        <Alert type="warning" showIcon title="等待审批" description="写入 /opt/app/app.yml 需要用户确认。" />
        <Alert type="error" showIcon title="权限不足" description="当前 AI 策略不允许执行写入工具。" />
        <Alert type="info" showIcon title="日志重连中" description="当前标签暂停输出，其他标签不受影响。" />
        <Alert type="success" showIcon title="MCP Server 运行中" description="127.0.0.1:8721/mcp 已就绪。" />
      </SectionGrid>
    </div>
  );
}

export function CoveragePage() {
  return (
    <div className="prototype-page">
      <PageHeader title="覆盖矩阵" description="按 PRD 和原型图核对界面覆盖率，当前目标为 100%。" actions={<Tag color="green">100% UI 覆盖</Tag>} />
      <Card><Table<CoverageRecord> rowKey="feature" pagination={false} dataSource={coverageRows} columns={[{ title: "功能点", dataIndex: "feature" }, { title: "覆盖页面", dataIndex: "page" }, { title: "状态", dataIndex: "status", render: (value) => <Tag color={value === "覆盖" ? "green" : "blue"}>{value}</Tag> }]} /></Card>
    </div>
  );
}
