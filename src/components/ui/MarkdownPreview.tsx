import type { ReactNode } from "react";

function renderInlineMarkdown(text: string) {
  const parts: ReactNode[] = [];
  const pattern =
    /(\[(?:tool:[A-Za-z0-9_:-]+|code:(?:\d+:file:\d+|snapshot:\d+:chunk:\d+)|citation:(?:code:(?:\d+:file:\d+|snapshot:\d+:chunk:\d+)|(?:chunk:)?\d+|(?:[A-Za-z0-9_-]+:)+chunk:\d+)|(?:[A-Za-z0-9_-]+:)+chunk:\d+)\]|\[[^\]]+\]\(https?:\/\/[^)]+\)|`[^`]+`|\*\*[^*]+\*\*|\*[^*]+\*)/g;
  let cursor = 0;
  for (const match of text.matchAll(pattern)) {
    const value = match[0];
    const start = match.index ?? 0;
    if (start > cursor) parts.push(text.slice(cursor, start));
    const fileCitation = value.match(/^\[code:(\d+):file:(\d+)\]$/);
    const chunkCitation = value.match(/^\[code:snapshot:(\d+):chunk:(\d+)\]$/);
    const wrappedFileCitation = value.match(
      /^\[citation:code:(\d+):file:(\d+)\]$/,
    );
    const wrappedChunkCitation = value.match(
      /^\[citation:code:snapshot:(\d+):chunk:(\d+)\]$/,
    );
    // AI Provider 的历史响应会使用 `citation:chunk:<id>`，新响应则简化为
    // `citation:<id>`；也兼容带完整 citation key 的包装格式。
    const providerChunkCitation = value.match(
      /^\[citation:(?:chunk:)?(\d+)\]$/,
    );
    const wrappedDocumentChunkCitation = value.match(
      /^\[citation:(?:[A-Za-z0-9_-]+:)+chunk:(\d+)\]$/,
    );
    const documentChunkCitation = value.match(
      /^\[(?:[A-Za-z0-9_-]+:)+chunk:(\d+)\]$/,
    );
    const toolCitation = value.match(/^\[(tool:[A-Za-z0-9_:-]+)\]$/);
    if (toolCitation) {
      parts.push(
        <span
          key={`${start}-tool-citation`}
          data-tool-citation={toolCitation[1]}
          aria-label="Git 实时证据"
          title="由本地只读 Git 工具生成的动态证据"
          className="mx-0.5 inline-flex rounded bg-[var(--bg-tertiary)] px-1.5 py-0.5 text-xs font-medium text-[var(--accent)]"
        >
          Git 实时证据
        </span>,
      );
    } else if (
      fileCitation ||
      chunkCitation ||
      wrappedFileCitation ||
      wrappedChunkCitation ||
      providerChunkCitation ||
      wrappedDocumentChunkCitation ||
      documentChunkCitation
    ) {
      const [, snapshotId, evidenceId] =
        fileCitation ??
        chunkCitation ??
        wrappedFileCitation ??
        wrappedChunkCitation ??
        [];
      const isProviderChunk =
        Boolean(providerChunkCitation) ||
        Boolean(
          (wrappedDocumentChunkCitation || documentChunkCitation) &&
          !wrappedChunkCitation &&
          !chunkCitation,
        );
      const citationId = isProviderChunk
        ? (providerChunkCitation ??
            wrappedDocumentChunkCitation ??
            documentChunkCitation)![1]
        : evidenceId;
      const isCodeCitation = Boolean(
        fileCitation ||
        chunkCitation ||
        wrappedFileCitation ||
        wrappedChunkCitation,
      );
      const evidenceType =
        isCodeCitation && (fileCitation || wrappedFileCitation)
          ? "文件"
          : "片段";
      const sourceLabel = isProviderChunk ? "证据片段" : "代码证据";
      const ariaLabel = isProviderChunk
        ? `证据片段 ${citationId}`
        : `代码证据：快照 ${snapshotId}，${evidenceType} ${citationId}`;
      const title = isProviderChunk
        ? `可追溯证据片段 #${citationId}`
        : `固定快照 #${snapshotId} 中的${evidenceType}证据 #${citationId}`;
      // 保留 Markdown 原文中的稳定引用键供后端确认时复核，但在阅读态转换为面向用户的
      // 证据标签，避免把内部快照/文件或片段标识误当成正文内容。
      parts.push(
        <span
          key={`${start}-citation`}
          data-code-citation={
            isProviderChunk
              ? `provider:chunk:${citationId}`
              : `${snapshotId}:${evidenceType}:${citationId}`
          }
          aria-label={ariaLabel}
          title={title}
          className="mx-0.5 inline-flex rounded bg-[var(--bg-tertiary)] px-1.5 py-0.5 text-xs font-medium text-[var(--accent)]"
        >
          {sourceLabel} · {evidenceType} #{citationId}
        </span>,
      );
    } else if (value.startsWith("`")) {
      parts.push(
        <code
          key={`${start}-code`}
          className="rounded bg-[var(--bg-tertiary)] px-1 py-0.5 font-mono text-[0.92em] text-[var(--text-primary)]"
        >
          {value.slice(1, -1)}
        </code>,
      );
    } else if (value.startsWith("**")) {
      parts.push(
        <strong
          key={`${start}-strong`}
          className="font-semibold text-[var(--text-primary)]"
        >
          {value.slice(2, -2)}
        </strong>,
      );
    } else if (value.startsWith("*")) {
      parts.push(<em key={`${start}-em`}>{value.slice(1, -1)}</em>);
    } else {
      const link = value.match(/^\[([^\]]+)\]\((https?:\/\/[^)]+)\)$/);
      parts.push(
        <a
          key={`${start}-link`}
          href={link?.[2]}
          target="_blank"
          rel="noreferrer"
          className="text-[var(--accent)] underline"
        >
          {link?.[1]}
        </a>,
      );
    }
    cursor = start + value.length;
  }
  if (cursor < text.length) parts.push(text.slice(cursor));
  return parts.length > 0 ? parts : text;
}

function parseMarkdownTableRow(line: string) {
  return line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((cell) => cell.trim());
}

function isMarkdownTableSeparator(line: string) {
  return /^\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?$/.test(line.trim());
}

function renderMarkdownBlocks(markdown: string) {
  const lines = markdown.replace(/\r\n/g, "\n").split("\n");
  const blocks: ReactNode[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];
    const trimmed = line.trim();
    if (!trimmed) {
      index += 1;
      continue;
    }
    if (trimmed.startsWith("```")) {
      const codeLines: string[] = [];
      index += 1;
      while (index < lines.length && !lines[index].trim().startsWith("```")) {
        codeLines.push(lines[index]);
        index += 1;
      }
      index += 1;
      blocks.push(
        <pre
          key={`code-${index}`}
          className="overflow-auto rounded border border-[var(--border)] bg-[var(--bg-secondary)] p-3 font-mono text-xs leading-5 text-[var(--text-primary)]"
        >
          <code>{codeLines.join("\n")}</code>
        </pre>,
      );
      continue;
    }
    if (
      index + 1 < lines.length &&
      isMarkdownTableSeparator(lines[index + 1])
    ) {
      const headers = parseMarkdownTableRow(line);
      index += 2;
      const rows: string[][] = [];
      while (index < lines.length && lines[index].includes("|")) {
        rows.push(parseMarkdownTableRow(lines[index]));
        index += 1;
      }
      blocks.push(
        <div key={`table-${index}`} className="max-w-full overflow-x-auto">
          <table className="w-full min-w-[720px] table-auto border-collapse border border-[var(--border)] text-left text-sm">
            <thead className="bg-[var(--bg-secondary)]">
              <tr>
                {headers.map((header, headerIndex) => (
                  <th
                    key={headerIndex}
                    className={`border border-[var(--border)] px-3 py-2 font-semibold break-words ${
                      headerIndex < 2 ? "min-w-32" : "min-w-64"
                    }`}
                  >
                    {renderInlineMarkdown(header)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, rowIndex) => (
                <tr key={rowIndex}>
                  {headers.map((_, cellIndex) => (
                    <td
                      key={cellIndex}
                      className={`border border-[var(--border)] px-3 py-2 align-top break-words ${
                        cellIndex < 2 ? "min-w-32" : "min-w-64"
                      }`}
                    >
                      {renderInlineMarkdown(row[cellIndex] ?? "")}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>,
      );
      continue;
    }
    const heading = trimmed.match(/^(#{1,6})\s+(.+)$/);
    if (heading) {
      const key = `heading-${index}`;
      const className =
        heading[1].length <= 2
          ? "m-0 text-lg font-semibold text-[var(--text-primary)]"
          : "m-0 text-base font-semibold text-[var(--text-primary)]";
      const content = renderInlineMarkdown(heading[2]);
      switch (heading[1].length) {
        case 1:
          blocks.push(
            <h1 key={key} className={className}>
              {content}
            </h1>,
          );
          break;
        case 2:
          blocks.push(
            <h2 key={key} className={className}>
              {content}
            </h2>,
          );
          break;
        case 3:
          blocks.push(
            <h3 key={key} className={className}>
              {content}
            </h3>,
          );
          break;
        case 4:
          blocks.push(
            <h4 key={key} className={className}>
              {content}
            </h4>,
          );
          break;
        case 5:
          blocks.push(
            <h5 key={key} className={className}>
              {content}
            </h5>,
          );
          break;
        default:
          blocks.push(
            <h6 key={key} className={className}>
              {content}
            </h6>,
          );
      }
      index += 1;
      continue;
    }
    if (/^[-*_]{3,}$/.test(trimmed)) {
      blocks.push(
        <hr key={`hr-${index}`} className="my-3 border-[var(--border)]" />,
      );
      index += 1;
      continue;
    }
    const quote = trimmed.match(/^>\s?(.+)$/);
    if (quote) {
      blocks.push(
        <blockquote
          key={`quote-${index}`}
          className="m-0 border-l-4 border-[var(--border)] pl-3 text-[var(--text-secondary)]"
        >
          {renderInlineMarkdown(quote[1])}
        </blockquote>,
      );
      index += 1;
      continue;
    }
    const listPattern = /^[-*+]\s+/.test(trimmed)
      ? /^[-*+]\s+/
      : /^\d+\.\s+/.test(trimmed)
        ? /^\d+\.\s+/
        : null;
    if (listPattern) {
      const items: string[] = [];
      while (index < lines.length && listPattern.test(lines[index].trim())) {
        items.push(lines[index].trim().replace(listPattern, ""));
        index += 1;
      }
      const List = /^\d+/.test(trimmed) ? "ol" : "ul";
      blocks.push(
        <List key={`list-${index}`} className="m-0 space-y-1 pl-5">
          {items.map((item, itemIndex) => (
            <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
          ))}
        </List>,
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
      !/^[-*+]\s+/.test(lines[index].trim()) &&
      !/^\d+\.\s+/.test(lines[index].trim()) &&
      !/^>\s?/.test(lines[index].trim()) &&
      !/^[-*_]{3,}$/.test(lines[index].trim())
    ) {
      paragraph.push(lines[index].trim());
      index += 1;
    }
    blocks.push(
      <p key={`paragraph-${index}`} className="m-0">
        {renderInlineMarkdown(paragraph.join(" "))}
      </p>,
    );
  }
  return blocks;
}

export function MarkdownPreview({
  content,
  testId,
}: {
  content: string;
  testId?: string;
}) {
  return (
    <article
      data-testid={testId}
      className="space-y-3 text-sm leading-7 text-[var(--text-primary)]"
    >
      {renderMarkdownBlocks(content)}
    </article>
  );
}
