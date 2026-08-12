import type { CSSProperties, ReactNode } from "react";
import { Card, Tag, Typography } from "antd";
import { Bot, ShieldAlert } from "lucide-react";
import type { RiskLevel } from "@/data/prototype";

const { Text, Title } = Typography;

const riskColor: Record<RiskLevel, string> = {
  L0: "green",
  L1: "blue",
  L2: "orange",
  L3: "red",
  readonly: "cyan",
  blocked: "red",
  ai: "purple",
};

interface PageHeaderProps {
  title: string;
  description: string;
  actions?: ReactNode;
}

export function PageHeader({ title, description, actions }: PageHeaderProps) {
  return (
    <div className="prototype-page-header">
      <div>
        <Title
          level={2}
          style={{ margin: 0, fontSize: 24, lineHeight: "32px" }}
        >
          {title}
        </Title>
        <Text type="secondary">{description}</Text>
      </div>
      {actions ? (
        <div className="prototype-header-actions">{actions}</div>
      ) : null}
    </div>
  );
}

interface StatCardProps {
  label: string;
  value: string;
  hint: string;
}

export function StatCard({ label, value, hint }: StatCardProps) {
  return (
    <Card className="prototype-card" size="small">
      <Text type="secondary">{label}</Text>
      <div className="prototype-stat-value">{value}</div>
      <Text type="secondary">{hint}</Text>
    </Card>
  );
}

export function RiskBadge({
  level,
  label,
}: {
  level: RiskLevel;
  label?: string;
}) {
  return <Tag color={riskColor[level]}>{label ?? level}</Tag>;
}

interface TerminalPanelProps {
  title?: string;
  lines: string[];
  footer?: ReactNode;
}

export function TerminalPanel({
  title = "terminal",
  lines,
  footer,
}: TerminalPanelProps) {
  return (
    <div className="prototype-terminal">
      <div className="prototype-terminal-title">{title}</div>
      {lines.map((line) => (
        <div
          key={line}
          className={
            line.includes("ERROR") || line.includes("拒绝")
              ? "terminal-error"
              : ""
          }
        >
          {line}
        </div>
      ))}
      {footer ? (
        <div className="prototype-terminal-footer">{footer}</div>
      ) : null}
    </div>
  );
}

interface AiInsightPanelProps {
  title?: string;
  children: ReactNode;
  tone?: "normal" | "warning";
  className?: string;
}

export function AiInsightPanel({
  title = "AI 建议",
  children,
  tone = "normal",
  className,
}: AiInsightPanelProps) {
  const cardClassName = [
    tone === "warning" ? "prototype-ai-card warning" : "prototype-ai-card",
    className,
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <Card
      size="small"
      className={cardClassName}
      title={
        <span className="prototype-card-title">
          {tone === "warning" ? <ShieldAlert size={16} /> : <Bot size={16} />}
          {title}
        </span>
      }
    >
      {children}
    </Card>
  );
}

interface CodeBlockProps {
  children: ReactNode;
  style?: CSSProperties;
}

export function CodeBlock({ children, style }: CodeBlockProps) {
  return (
    <pre className="prototype-code" style={style}>
      {children}
    </pre>
  );
}

export function TwoColumn({
  left,
  right,
}: {
  left: ReactNode;
  right: ReactNode;
}) {
  return (
    <div className="prototype-two-column">
      <div>{left}</div>
      <div>{right}</div>
    </div>
  );
}

export function SectionGrid({
  children,
  columns = 3,
}: {
  children: ReactNode;
  columns?: 2 | 3 | 4;
}) {
  return (
    <div className={`prototype-grid prototype-grid-${columns}`}>{children}</div>
  );
}
