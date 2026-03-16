#!/usr/bin/env node
// render-animations.js — Convert animation frame JSON to SVG files.
//
// Usage: node scripts/render-animations.js [name]
//   Reads: target/animations/{name}.json
//   Writes: target/animations/{name}/frame-{NNN}.svg
//
// If no name given, processes all .json files in target/animations/.

const fs = require("fs");
const path = require("path");

// --- Visual constants ---
const CELL_W = 8; // px per character
const CELL_H = 17; // px per character line
const PADDING = 8;
const CAPTION_H = 28;

const COLORS = {
  bg: "#1a1a2e",
  fg: "#e0e0e0",
  faded: "#6a6a8a",
  cursorBg: "#6c63ff",
  cursorFg: "#ffffff",
  cursorLine: "#252540",
  statusBg: "#2a2a4a",
  statusFg: "#b0b0cc",
  border: "#3a3a5a",
  captionFg: "#a0a0a0",
  diffAdded: "#4ec970",
  diffRemoved: "#e06060",
};

function escapeXml(text) {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function renderFrame(frame) {
  const pane = frame.frame.panes[0];
  if (!pane) return null;

  const lines = pane.visible_lines || [];
  const cursor = pane.cursor || { line: 0, column: 0 };
  const statusBar = pane.status_bar;

  const cols = frame.width || 80;
  const rows = frame.height || 24;
  const hasCaption = !!frame.caption;

  const contentW = cols * CELL_W;
  const contentH = rows * CELL_H;
  const totalW = contentW + PADDING * 2;
  const totalH = contentH + PADDING * 2 + (hasCaption ? CAPTION_H : 0);

  let svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${totalW}" height="${totalH}" viewBox="0 0 ${totalW} ${totalH}">\n`;
  svg += `<rect width="${totalW}" height="${totalH}" fill="${COLORS.bg}" rx="6"/>\n`;

  // Cursor line highlight
  const cursorLineY = PADDING + cursor.line * CELL_H;
  svg += `<rect x="${PADDING}" y="${cursorLineY}" width="${contentW}" height="${CELL_H}" fill="${COLORS.cursorLine}"/>\n`;

  // Text lines
  for (let i = 0; i < lines.length && i < rows - 1; i++) {
    const line = lines[i];
    const text = line.text || "";
    const y = PADDING + i * CELL_H + 13; // baseline offset

    if (line.spans && line.spans.length > 0) {
      // Render with span styling
      let x = PADDING;
      for (const span of line.spans) {
        const spanText = text.slice(span.range[0], span.range[1]) || "";
        if (!spanText) continue;
        const color = spanStyleToColor(span.style);
        svg += `<text x="${x}" y="${y}" fill="${color}" font-family="monospace" font-size="13">${escapeXml(spanText)}</text>\n`;
        x += spanText.length * CELL_W;
      }
    } else {
      svg += `<text x="${PADDING}" y="${y}" fill="${COLORS.fg}" font-family="monospace" font-size="13">${escapeXml(text)}</text>\n`;
    }
  }

  // Block cursor
  const cx = PADDING + cursor.column * CELL_W;
  const cy = PADDING + cursor.line * CELL_H;
  svg += `<rect x="${cx}" y="${cy}" width="${CELL_W}" height="${CELL_H}" fill="${COLORS.cursorBg}" opacity="0.8"/>\n`;
  // Character under cursor
  const cursorLine = lines[cursor.line];
  if (cursorLine) {
    const ch = (cursorLine.text || "")[cursor.column] || " ";
    svg += `<text x="${cx}" y="${cy + 13}" fill="${COLORS.cursorFg}" font-family="monospace" font-size="13">${escapeXml(ch)}</text>\n`;
  }

  // Status bar (bottom line)
  const statusY = PADDING + (rows - 1) * CELL_H;
  svg += `<rect x="${PADDING}" y="${statusY}" width="${contentW}" height="${CELL_H}" fill="${COLORS.statusBg}"/>\n`;
  if (statusBar) {
    const mode = statusBar.mode_badge || "";
    const file = statusBar.filename || "";
    const pos = statusBar.cursor_pos || "";
    const statusText = `${mode}  ${file}  ${pos}`;
    svg += `<text x="${PADDING + 4}" y="${statusY + 13}" fill="${COLORS.statusFg}" font-family="monospace" font-size="13">${escapeXml(statusText)}</text>\n`;
  }

  // Border
  svg += `<rect x="${PADDING}" y="${PADDING}" width="${contentW}" height="${contentH}" fill="none" stroke="${COLORS.border}" stroke-width="1" rx="2"/>\n`;

  // Caption
  if (hasCaption) {
    const captionY = PADDING + contentH + 20;
    svg += `<text x="${totalW / 2}" y="${captionY}" fill="${COLORS.captionFg}" font-family="monospace" font-size="12" text-anchor="middle">${escapeXml(frame.caption)}</text>\n`;
  }

  svg += `</svg>`;
  return svg;
}

function spanStyleToColor(style) {
  switch (style) {
    case "Heading1":
    case "Heading2":
    case "Heading3":
      return "#c0c0ff";
    case "Bold":
      return "#ffffff";
    case "Italic":
      return "#d0d0d0";
    case "Link":
    case "WikiLink":
      return "#6c9fff";
    case "Tag":
      return "#c080ff";
    case "DueDate":
      return "#f0c040";
    case "TaskOpen":
    case "TaskDone":
      return "#80c0a0";
    case "BlockId":
    case "SyntaxNoise":
      return COLORS.faded;
    case "DiffAdded":
      return COLORS.diffAdded;
    case "DiffRemoved":
      return COLORS.diffRemoved;
    case "Blockquote":
      return "#8080a0";
    default:
      return COLORS.fg;
  }
}

// --- Main ---
function processAnimation(jsonPath) {
  const name = path.basename(jsonPath, ".json");
  const frames = JSON.parse(fs.readFileSync(jsonPath, "utf-8"));
  const outDir = path.join(path.dirname(jsonPath), name);
  fs.mkdirSync(outDir, { recursive: true });

  let rendered = 0;
  for (const frame of frames) {
    const svg = renderFrame(frame);
    if (svg) {
      const filename = `frame-${String(frame.index).padStart(3, "0")}.svg`;
      fs.writeFileSync(path.join(outDir, filename), svg);
      rendered++;
    }
  }

  // Write timing metadata for GIF assembly
  const timing = frames.map((f) => ({
    index: f.index,
    duration_ms: f.duration_ms,
    caption: f.caption,
  }));
  fs.writeFileSync(path.join(outDir, "timing.json"), JSON.stringify(timing, null, 2));

  console.log(`${name}: ${rendered} SVG frames → ${outDir}`);
}

const animDir = path.resolve(__dirname, "..", "target", "animations");
const args = process.argv.slice(2);

if (args.length > 0) {
  for (const name of args) {
    processAnimation(path.join(animDir, `${name}.json`));
  }
} else {
  if (!fs.existsSync(animDir)) {
    console.error(`No animations found. Run: cargo test -p bloom-test-harness --test animations`);
    process.exit(1);
  }
  const files = fs.readdirSync(animDir).filter((f) => f.endsWith(".json"));
  if (files.length === 0) {
    console.error("No .json files in target/animations/");
    process.exit(1);
  }
  for (const file of files) {
    processAnimation(path.join(animDir, file));
  }
}
