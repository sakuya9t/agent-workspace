/* eslint-disable i18next/no-literal-string -- Markdown source text and internal
   React keys appear inside JSX; actual UI labels still come from i18n. */
import type { ReactNode } from "react";
import i18n from "../i18n";

interface InlineToken {
  kind: "code" | "image" | "link" | "strong" | "strike" | "emphasis" | "autolink";
  match: RegExpExecArray;
  priority: number;
}

const INLINE_PATTERNS: Array<{
  kind: InlineToken["kind"];
  pattern: RegExp;
}> = [
  { kind: "code", pattern: /`([^`\n]+)`/ },
  {
    kind: "image",
    pattern: /!\[([^\]]*)\]\(([^)\s]+)(?:\s+["']([^"']*)["'])?\)/,
  },
  {
    kind: "link",
    pattern: /\[([^\]]+)\]\(([^)\s]+)(?:\s+["']([^"']*)["'])?\)/,
  },
  { kind: "strong", pattern: /\*\*([^*\n]+)\*\*|__([^_\n]+)__/ },
  { kind: "strike", pattern: /~~([^~\n]+)~~/ },
  { kind: "emphasis", pattern: /\*([^*\n]+)\*|_([^_\n]+)_/ },
  { kind: "autolink", pattern: /<(https?:\/\/[^>\s]+|mailto:[^>\s]+)>/ },
];

/** A safe, dependency-free Markdown preview for repository text.
 *
 * Markdown is turned directly into React elements; raw HTML remains text rather
 * than entering the DOM. This covers the document constructs most useful while
 * reviewing a README: headings, lists, task lists, quotes, tables, fenced code,
 * links, images, and inline emphasis/code.
 */
export function MarkdownPreview({ content }: { content: string }) {
  return <article className="markdown-preview">{renderBlocks(content)}</article>;
}

function renderBlocks(source: string): ReactNode[] {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const blocks: ReactNode[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i += 1;
      continue;
    }

    const fence = line.match(/^ {0,3}(`{3,}|~{3,})\s*([\w+-]*)?.*$/);
    if (fence) {
      const marker = fence[1][0];
      const length = fence[1].length;
      const code: string[] = [];
      i += 1;
      while (i < lines.length && !new RegExp(`^ {0,3}${marker}{${length},}\\s*$`).test(lines[i])) {
        code.push(lines[i]);
        i += 1;
      }
      if (i < lines.length) i += 1;
      const language = fence[2] ? `language-${fence[2]}` : undefined;
      blocks.push(
        <pre key={`code-${i}`}>
          <code className={language}>{code.join("\n")}</code>
        </pre>,
      );
      continue;
    }

    const atxHeading = line.match(/^ {0,3}(#{1,6})\s+(.+?)\s*#*\s*$/);
    if (atxHeading) {
      const level = atxHeading[1].length;
      const children = renderInline(atxHeading[2], `heading-${i}`);
      blocks.push(heading(level, children, `heading-${i}`));
      i += 1;
      continue;
    }

    const setext = i + 1 < lines.length ? lines[i + 1].match(/^ {0,3}(=+|-+)\s*$/) : null;
    if (setext) {
      blocks.push(heading(setext[1][0] === "=" ? 1 : 2, renderInline(line.trim(), `heading-${i}`), `heading-${i}`));
      i += 2;
      continue;
    }

    if (/^ {0,3}(?:(?:\*\s*){3,}|(?:-\s*){3,}|(?:_\s*){3,})$/.test(line)) {
      blocks.push(<hr key={`hr-${i}`} />);
      i += 1;
      continue;
    }

    if (/^ {0,3}>/.test(line)) {
      const quote: string[] = [];
      const start = i;
      while (i < lines.length) {
        const match = lines[i].match(/^ {0,3}>\s?(.*)$/);
        if (!match) break;
        quote.push(match[1]);
        i += 1;
      }
      blocks.push(<blockquote key={`quote-${start}`}>{renderBlocks(quote.join("\n"))}</blockquote>);
      continue;
    }

    const listItem = line.match(/^ {0,3}([-+*]|\d+[.)])\s+(.+)$/);
    if (listItem) {
      const ordered = /^\d/.test(listItem[1]);
      const items: Array<{ text: string; checked?: boolean }> = [];
      const start = i;
      while (i < lines.length) {
        const item = lines[i].match(/^ {0,3}([-+*]|\d+[.)])\s+(.+)$/);
        if (!item || /^\d/.test(item[1]) !== ordered) break;
        let text = item[2];
        i += 1;
        while (i < lines.length) {
          const continuation = lines[i].match(/^ {2,}(\S.*)$/);
          if (!continuation || /^ {0,3}([-+*]|\d+[.)])\s+/.test(lines[i])) break;
          text += ` ${continuation[1]}`;
          i += 1;
        }
        const task = text.match(/^\[([ xX])\]\s+(.+)$/);
        items.push(task ? { text: task[2], checked: task[1].toLowerCase() === "x" } : { text });
      }
      const children = items.map((item, index) => (
        <li className={item.checked === undefined ? undefined : "task-list-item"} key={index}>
          {item.checked !== undefined && (
            <input
              type="checkbox"
              checked={item.checked}
              disabled
              aria-label={i18n.t(
                item.checked ? "diffModal.taskComplete" : "diffModal.taskIncomplete",
              )}
            />
          )}
          {renderInline(item.text, `list-${start}-${index}`)}
        </li>
      ));
      blocks.push(
        ordered ? (
          <ol key={`list-${start}`}>{children}</ol>
        ) : (
          <ul className={items.some((item) => item.checked !== undefined) ? "task-list" : undefined} key={`list-${start}`}>
            {children}
          </ul>
        ),
      );
      continue;
    }

    if (i + 1 < lines.length && isTableDelimiter(lines[i + 1])) {
      const headers = tableCells(line);
      const delimiters = tableCells(lines[i + 1]);
      if (headers.length === delimiters.length) {
        const start = i;
        const alignments = delimiters.map((cell) => {
          const value = cell.trim();
          if (value.startsWith(":") && value.endsWith(":")) return "center";
          if (value.endsWith(":")) return "right";
          return "left";
        });
        i += 2;
        const rows: string[][] = [];
        while (i < lines.length && lines[i].includes("|") && lines[i].trim()) {
          const cells = tableCells(lines[i]);
          while (cells.length < headers.length) cells.push("");
          rows.push(cells.slice(0, headers.length));
          i += 1;
        }
        blocks.push(
          <div className="markdown-table-wrap" key={`table-${start}`}>
            <table>
              <thead>
                <tr>
                  {headers.map((cell, index) => (
                    <th key={index} style={{ textAlign: alignments[index] }}>
                      {renderInline(cell.trim(), `table-${start}-head-${index}`)}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {rows.map((row, rowIndex) => (
                  <tr key={rowIndex}>
                    {row.map((cell, cellIndex) => (
                      <td key={cellIndex} style={{ textAlign: alignments[cellIndex] }}>
                        {renderInline(cell.trim(), `table-${start}-${rowIndex}-${cellIndex}`)}
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
    }

    const paragraph: string[] = [];
    const start = i;
    while (i < lines.length && lines[i].trim() && !isBlockStart(lines, i, start)) {
      paragraph.push(lines[i]);
      i += 1;
    }
    // A defensive fallback for any new block syntax added above.
    if (paragraph.length === 0) {
      paragraph.push(lines[i]);
      i += 1;
    }
    blocks.push(<p key={`paragraph-${start}`}>{renderParagraph(paragraph, `paragraph-${start}`)}</p>);
  }

  return blocks;
}

function isBlockStart(lines: string[], index: number, paragraphStart: number): boolean {
  if (index === paragraphStart) return false;
  const line = lines[index];
  return (
    /^ {0,3}(`{3,}|~{3,})/.test(line) ||
    /^ {0,3}#{1,6}\s+/.test(line) ||
    /^ {0,3}>/.test(line) ||
    /^ {0,3}([-+*]|\d+[.)])\s+/.test(line) ||
    /^ {0,3}(?:(?:\*\s*){3,}|(?:-\s*){3,}|(?:_\s*){3,})$/.test(line) ||
    (index + 1 < lines.length && isTableDelimiter(lines[index + 1]))
  );
}

function heading(level: number, children: ReactNode[], key: string): ReactNode {
  switch (level) {
    case 1:
      return <h1 key={key}>{children}</h1>;
    case 2:
      return <h2 key={key}>{children}</h2>;
    case 3:
      return <h3 key={key}>{children}</h3>;
    case 4:
      return <h4 key={key}>{children}</h4>;
    case 5:
      return <h5 key={key}>{children}</h5>;
    default:
      return <h6 key={key}>{children}</h6>;
  }
}

function renderParagraph(lines: string[], keyPrefix: string): ReactNode[] {
  const result: ReactNode[] = [];
  lines.forEach((line, index) => {
    const hardBreak = / {2,}$/.test(line) || /\\$/.test(line);
    const text = hardBreak ? line.replace(/(?: {2,}|\\)$/, "") : line;
    result.push(...renderInline(text.trim(), `${keyPrefix}-${index}`));
    if (index < lines.length - 1) {
      result.push(hardBreak ? <br key={`${keyPrefix}-br-${index}`} /> : " ");
    }
  });
  return result;
}

function renderInline(text: string, keyPrefix: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  let remaining = text;
  let tokenIndex = 0;

  while (remaining) {
    let next: InlineToken | null = null;
    INLINE_PATTERNS.forEach(({ kind, pattern }, priority) => {
      const match = pattern.exec(remaining);
      if (
        match &&
        (!next || match.index < next.match.index || (match.index === next.match.index && priority < next.priority))
      ) {
        next = { kind, match, priority };
      }
    });

    if (!next) {
      nodes.push(remaining);
      break;
    }

    const token: InlineToken = next;
    if (token.match.index > 0) nodes.push(remaining.slice(0, token.match.index));
    const key = `${keyPrefix}-${tokenIndex}`;
    const inner = token.match[1] ?? token.match[2] ?? "";

    switch (token.kind) {
      case "code":
        nodes.push(<code key={key}>{inner}</code>);
        break;
      case "image": {
        const src = safeUrl(token.match[2], true);
        nodes.push(
          src ? (
            <img key={key} src={src} alt={token.match[1]} title={token.match[3]} loading="lazy" />
          ) : (
            token.match[1]
          ),
        );
        break;
      }
      case "link": {
        const href = safeUrl(token.match[2], false);
        const external = Boolean(href && /^https?:\/\//i.test(href));
        nodes.push(
          href ? (
            <a
              key={key}
              href={href}
              title={token.match[3]}
              target={external ? "_blank" : undefined}
              rel={external ? "noreferrer" : undefined}
            >
              {renderInline(token.match[1], `${key}-link`)}
            </a>
          ) : (
            token.match[1]
          ),
        );
        break;
      }
      case "strong":
        nodes.push(<strong key={key}>{renderInline(inner, `${key}-strong`)}</strong>);
        break;
      case "strike":
        nodes.push(<del key={key}>{renderInline(inner, `${key}-strike`)}</del>);
        break;
      case "emphasis":
        nodes.push(<em key={key}>{renderInline(inner, `${key}-em`)}</em>);
        break;
      case "autolink": {
        const href = safeUrl(inner, false);
        nodes.push(
          href ? (
            <a
              key={key}
              href={href}
              target={href.startsWith("http") ? "_blank" : undefined}
              rel={href.startsWith("http") ? "noreferrer" : undefined}
            >
              {inner}
            </a>
          ) : (
            inner
          ),
        );
        break;
      }
    }

    remaining = remaining.slice(token.match.index + token.match[0].length);
    tokenIndex += 1;
  }

  return nodes;
}

function safeUrl(value: string, image: boolean): string | undefined {
  const url = value.trim().replace(/[\u0000-\u001f\u007f]/g, "");
  if (
    /^(?:https?:\/\/|mailto:|#|\/(?!\/)|\.{1,2}\/)/i.test(url) ||
    (!/^[a-z][a-z\d+.-]*:/i.test(url) && !url.startsWith("//"))
  ) {
    return url;
  }
  if (image && /^data:image\/(?:png|jpeg|gif|webp);base64,/i.test(url)) return url;
  return undefined;
}

function isTableDelimiter(line: string): boolean {
  const cells = tableCells(line);
  return cells.length > 0 && cells.every((cell) => /^:?-{3,}:?$/.test(cell.trim()));
}

function tableCells(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|");
}
