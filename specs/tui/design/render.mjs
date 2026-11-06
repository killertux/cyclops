// Renders the design in source/ to terminal cells, and writes screens/ and tokens.json.
//
//   node specs/tui/design/render.mjs              text and cell files
//   CHROME=/path/to/chrome node ... render.mjs    also the PNG screenshots
//   FIREFOX=/path/to/firefox node ... render.mjs  the same, with Firefox
//   ONLY=alerts,why ... render.mjs                only those PNG screenshots
//
// source/ is drawn for a canvas, with CSS lines. This file turns each screen into the
// glyphs a terminal would show, so screens/ is what an implementation should match.

import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const SOURCE = join(HERE, 'source');
const SCREENS = join(HERE, 'screens');
const CELL_W = 9;
const CELL_H = 18;

const BOARDS = [
  ['overview', 'Overview', { indexing: 'elapsed' }],
  ['overview-percent', 'Overview', { indexing: 'percent' }],
  ['overview-idle', 'Overview', { indexing: 'idle' }],
  ['map', 'Map', {}],
  ['explore', 'Explore', {}],
  ['why', 'Why', {}],
  ['why-cohesion', 'WhyCohesion', {}],
  ['why-forms', 'WhyForms', {}],
  ['diagram', 'Diagram', {}],
  ['diagram-types', 'DiagramTypes', {}],
  ['more-nodes', 'Popups', { popup: 'more' }],
  ['relation-picker', 'Popups', { popup: 'relations' }],
  ['metric-picker', 'Popups', { popup: 'metrics' }],
  ['find', 'Find', {}],
  ['alerts', 'Alerts', {}],
  ['alert-rule', 'AlertRule', {}],
  ['status', 'Status', { confirm: false }],
  ['status-running', 'Status', { confirm: false, running: true }],
  ['status-confirm', 'Status', { confirm: true }],
  ['first-run', 'FirstRun', {}],
  ['space-menu', 'Keys', {}],
  ['context-menu', 'Menu', {}],
  ['small-terminal', 'Narrow', {}],
  ['mascot', 'Mascot', {}],
  ['visual-language', 'Term', {}],
];

const read = (name) => readFileSync(join(SOURCE, `${name}.dc.html`), 'utf8');

function loadBoard(name, props) {
  const source = read(name);
  if (name === 'Term') {
    const demo = source.match(/const DEMO = (\[[\s\S]*?\n {4}\]);/);
    return { ops: new Function(`return ${demo[1]};`)(), cols: 120, rows: 36 };
  }
  const code = source.match(/class Component extends DCLogic \{[\s\S]*?\n\}\n<\/script>/)[0].replace('</script>', '');
  const Component = new Function('DCLogic', `${code}; return Component;`)(
    class {
      constructor(p) {
        this.props = p;
      }
    },
  );
  const vals = new Component(props).renderVals();
  return { ops: vals.ops, cols: vals.cols ?? 120, rows: vals.rows ?? 36 };
}

function loadTokens() {
  const source = read('Term');
  const constants = source.match(/const CW = 9[^\n]*;/)[0];
  const table = (name) => source.match(new RegExp(`const ${name} = (\\{[\\s\\S]*?\\n {4}\\});`))[1];
  const pixels = new Function(`return ${source.match(/const PX = (\{[^\n]*\});/)[1]};`)();
  const theme = (light, name) => {
    const raw = new Function('light', `${constants}; return { t: ${table(name)}, BG, SEL, LINE, TRACK };`)(light);
    const tokens = {};
    for (const [key, [fg, bg, bold, underline]] of Object.entries(raw.t)) {
      tokens[key] = { fg, bg: bg ?? null, bold: !!bold, underline: !!underline };
    }
    tokens.line = { fg: raw.LINE, bg: null, bold: false, underline: false };
    tokens.track = { fg: raw.TRACK, bg: null, bold: false, underline: false };
    tokens.sel = { fg: null, bg: raw.SEL, bold: false, underline: false };
    for (const [key, color] of Object.entries(pixels)) {
      tokens[`px.${key}`] = { fg: color, bg: color, bold: false, underline: false };
    }
    return { ground: raw.BG, tokens };
  };
  return { dark: theme(false, 'DARK'), light: theme(true, 'LIGHT') };
}

const N = 1;
const S = 2;
const W = 4;
const E = 8;
const LIGHT_LINES = { [W | E]: '─', [N | S]: '│', [S | E]: '┌', [S | W]: '┐', [N | E]: '└', [N | W]: '┘', [N | S | E]: '├', [N | S | W]: '┤', [S | W | E]: '┬', [N | W | E]: '┴', [N | S | W | E]: '┼' };
const HEAVY_LINES = { [W | E]: '━', [N | S]: '┃', [S | E]: '┏', [S | W]: '┓', [N | E]: '┗', [N | W]: '┛', [N | S | E]: '┣', [N | S | W]: '┫', [S | W | E]: '┳', [N | W | E]: '┻', [N | S | W | E]: '╋' };
const LINES = {
  s: LIGHT_LINES,
  B: HEAVY_LINES,
  d: { ...LIGHT_LINES, [W | E]: '╌', [N | S]: '╎' },
  o: { ...LIGHT_LINES, [W | E]: '┄', [N | S]: '┆' },
};
const LINE_RANK = { o: 0, d: 1, s: 2, B: 3 };
// Stops at ▇ so that spreads on neighboring rows never join up.
const SPARKS = '▁▂▃▄▅▆▇▇';
const TABS = ['Overview', 'Map', 'Explore', 'Diagram', 'Find', 'Alerts', 'Status'];

function drawScreen({ ops, cols, rows }, tokens) {
  const grid = Array.from({ length: rows }, () => Array.from({ length: cols }, () => ({ c: ' ', k: 'fg', bg: null, solid: false })));
  const lines = new Map();
  const at = (x, y) => grid[y]?.[x];

  const parse = (text, base) => {
    const out = [];
    const re = /⟦(\w+)\|([^⟧]*)⟧/g;
    let start = 0;
    let m;
    while ((m = re.exec(text))) {
      for (const ch of text.slice(start, m.index)) out.push([ch, base]);
      for (const ch of m[2]) out.push([ch, m[1]]);
      start = re.lastIndex;
    }
    for (const ch of text.slice(start)) out.push([ch, base]);
    return out;
  };
  const put = (x, y, text, k = 'fg') => {
    parse(String(text), k).forEach(([ch, token], i) => {
      const cell = at(x + i, y);
      if (!cell) return;
      cell.c = ch;
      cell.k = token;
      if (tokens[token]?.bg) cell.bg = token;
    });
  };
  const rput = (x, y, text, k = 'fg') => put(x - parse(String(text), k).length + 1, y, text, k);
  const fill = (x, y, w, h, k) => {
    for (let j = y; j < y + h; j++) {
      for (let i = x; i < x + w; i++) {
        const cell = at(i, j);
        if (cell) Object.assign(cell, { c: ' ', k, bg: k, solid: true });
      }
    }
  };
  const highlight = (x, y, w, bg = 'sel') => {
    for (let i = x; i < x + w; i++) {
      const cell = at(i, y);
      if (!cell) continue;
      if (bg === 'ground') cell.solid = true;
      else if (bg === 'sel') cell.bg = 'sel';
    }
  };
  const link = (x, y, bit, style, k) => {
    const key = `${x},${y}`;
    const old = lines.get(key) ?? { mask: 0, style, k };
    old.mask |= bit;
    if (LINE_RANK[style] > LINE_RANK[old.style]) Object.assign(old, { style, k });
    lines.set(key, old);
  };
  const line = (x1, y1, x2, y2, style = 's', k = 'dim') => {
    if (y1 === y2) {
      const [a, b] = [Math.min(x1, x2), Math.max(x1, x2)];
      for (let x = a; x <= b; x++) link(x, y1, (x > a ? W : 0) | (x < b ? E : 0), style, k);
    } else {
      const [a, b] = [Math.min(y1, y2), Math.max(y1, y2)];
      for (let y = a; y <= b; y++) link(x1, y, (y > a ? N : 0) | (y < b ? S : 0), style, k);
    }
  };
  const box = (x, y, w, h, title, mode) => {
    const k = mode === 'p' ? 'popk' : mode === 'a' || mode === 'f' ? 'acc' : 'line';
    const [h1, v1, nw, ne, sw, se] = mode === 'f' ? '━┃┏┓┗┛' : '─│╭╮╰╯';
    const glyphs = [];
    for (let i = 1; i < w - 1; i++) glyphs.push([x + i, y, h1], [x + i, y + h - 1, h1]);
    for (let j = 1; j < h - 1; j++) glyphs.push([x, y + j, v1], [x + w - 1, y + j, v1]);
    glyphs.push([x, y, nw], [x + w - 1, y, ne], [x, y + h - 1, sw], [x + w - 1, y + h - 1, se]);
    if (mode === 'p') {
      fill(x, y, w, h, 'pop');
      for (const [gx, gy, ch] of glyphs) put(gx, gy, ch, k);
      return;
    }
    for (const [gx, gy, ch] of glyphs) lines.set(`${gx},${gy}`, { glyph: ch, k });
    if (title) {
      const text = ` ${title} `;
      put(x + 2, y, text, mode === 'a' ? 'accb' : 'dim');
      highlight(x + 2, y, parse(text, 'dim').length, 'ground');
    }
  };
  const rule = (x, y, w, mode) => {
    const k = mode === 'f' ? 'acc' : 'line';
    for (let i = 0; i < w; i++) {
      const glyph = i === 0 ? (mode === 'f' ? '┠' : '├') : i === w - 1 ? (mode === 'f' ? '┨' : '┤') : '─';
      lines.set(`${x + i},${y}`, { glyph, k });
    }
  };
  const table = (x, y, widths, tableRows, options = {}) => {
    tableRows.forEach((row, i) => {
      let cx = x;
      const base = options.head && i === 0 ? 'faint' : 'fg';
      row.forEach((value, j) => {
        const width = Math.abs(widths[j]);
        const text = String(value);
        const length = parse(text, base).length;
        put(cx + (widths[j] < 0 ? Math.max(0, width - length) : 0), y + i, text, base);
        cx += width + 1;
      });
      if (options.sel === i) highlight(x, y + i, widths.reduce((sum, w) => sum + Math.abs(w) + 1, -1));
    });
  };
  const spark = (x, y, values, k = 'dim', mark = -1, markToken = 'h2') => {
    values.forEach((v, i) => {
      if (v > 0) put(x + i, y, SPARKS[v - 1], i === mark ? markToken : k);
    });
  };
  const bar = (x, y, w, fraction, k = 'h1') => {
    const position = w * fraction;
    for (let i = 0; i < w; i++) {
      if (i + 1 <= position) put(x + i, y, '━', k);
      else if (i + 0.5 <= position) put(x + i, y, '╸', k);
      else put(x + i, y, '─', 'track');
    }
  };
  const pixels = (x, y, art, scale = 1) => {
    const half = new Map();
    art.forEach((row, j) => {
      [...row].forEach((ch, i) => {
        if (ch === '.') return;
        for (let dx = 0; dx < scale; dx++) {
          for (let dy = 0; dy < scale; dy++) half.set(`${x + i * scale + dx},${y * 2 + j * scale + dy}`, ch);
        }
      });
    });
    const height = Math.ceil((art.length * scale) / 2);
    const width = art[0].length * scale;
    for (let j = y; j < y + height; j++) {
      for (let i = x; i < x + width; i++) {
        const top = half.get(`${i},${j * 2}`);
        const bottom = half.get(`${i},${j * 2 + 1}`);
        const cell = at(i, j);
        if (!cell || (!top && !bottom)) continue;
        if (top && bottom && top === bottom) Object.assign(cell, { c: '█', k: `px.${top}` });
        else if (top) Object.assign(cell, { c: '▀', k: `px.${top}`, bg: bottom ? `px.${bottom}` : null });
        else Object.assign(cell, { c: '▄', k: `px.${bottom}` });
      }
    }
  };
  const shell = (o) => {
    put(2, 0, '◉', 'accb');
    put(4, 0, 'cyclops', 'b');
    let x = 12;
    for (const tab of TABS) {
      if (tab === o.v) put(x, 0, ` ${tab} `, 'on');
      else put(x, 0, ` ⟦key|${tab[0]}⟧${tab.slice(1)} `, 'dim');
      x += tab.length + 2;
    }
    if (o.proj !== '') rput(cols - 2, 0, o.proj ?? 'shop · 842 files · 96,410 lines', 'faint');
    if (o.crumb) put(1, 1, o.crumb, 'dim');
    if (o.side) rput(cols - 2, 1, o.side, 'faint');
    const y = rows - 2;
    fill(0, y, cols, 1, 'chip');
    const mode = ` ${o.mode ?? 'NORMAL'} `;
    put(0, y, mode, o.mk ?? 'mode');
    const filter = o.filter ? `⟦chipa|${o.filter}⟧` : '⟦chipd|none⟧';
    const lens = o.lens ?? `metric ⟦chipa|${o.metric ?? 'cyclomatic'}⟧  filter ${filter}  tests ${o.tests ?? 'off'}  layer ${o.layer ?? 'syntax'}`;
    put(mode.length + 1, y, lens, 'chip');
    rput(cols - 2, y, o.right ?? '⟦chiph|29 alerts⟧  2 problems  ⟦chipd|fresh⟧', 'chip');
    if (o.cmd) put(0, rows - 1, o.cmd, 'fg');
    else if (o.hint) {
      let hx = 1;
      for (const [key, label] of o.hint) {
        put(hx, rows - 1, `⟦acc|${key}⟧ ${label}`, 'dim');
        hx += key.length + label.length + 3;
      }
      if (o.menu !== false) rput(cols - 2, rows - 1, '⟦acc|Space⟧ menu   ⟦acc|?⟧ keys', 'dim');
    }
  };

  const draw = { t: put, r: rput, b: box, hr: rule, l: line, f: fill, h: highlight, tb: table, sh: shell, sp: spark, bar, px: pixels };
  for (const [name, ...args] of ops) draw[name]?.(...args);

  for (const [key, entry] of lines) {
    const [x, y] = key.split(',').map(Number);
    const cell = at(x, y);
    if (!cell || cell.c !== ' ' || cell.solid) continue;
    const mask = entry.mask === W || entry.mask === E ? W | E : entry.mask === N || entry.mask === S ? N | S : entry.mask;
    Object.assign(cell, { c: entry.glyph ?? LINES[entry.style][mask], k: entry.k });
  }
  return grid;
}

function toRuns(grid) {
  return grid.map((row) => {
    const runs = [];
    row.forEach((cell, x) => {
      const last = runs[runs.length - 1];
      if (last && last.style === cell.k && last.bg === cell.bg) last.text += cell.c;
      else runs.push({ x, text: cell.c, style: cell.k, bg: cell.bg });
    });
    return runs.filter((run) => run.bg || run.text.trim()).map((run) => (run.bg ? [run.x, run.text, run.style, run.bg] : [run.x, run.text, run.style]));
  });
}

const escapeHtml = (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[ch] ?? ch;

// Headless Firefox ignores its scale setting, so the page zooms itself. It also paints a
// cell's background over the line glyphs of the row above, which leaves gaps in the borders
// of popups, so backgrounds and glyphs are drawn as two layers.
function firefoxScreenshot(firefox, html, file, cols, rows) {
  const dir = mkdtempSync(join(tmpdir(), 'cyclops-design-'));
  const page = join(dir, 'screen.html');
  const profile = join(dir, 'profile');
  mkdirSync(profile);
  const styleEnd = html.lastIndexOf('</style>');
  const grid = html.slice(styleEnd + '</style>'.length);
  const layers = `html{zoom:2;overflow:hidden}
section{position:absolute;left:0;top:0}
.ground i{color:transparent!important;text-decoration-color:transparent!important}
.glyphs i{background:none!important}`;
  writeFileSync(page, `${html.slice(0, styleEnd)}${layers}</style><section class="ground">${grid}</section><section class="glyphs">${grid}</section>`);
  execFileSync(firefox, ['--headless', '--no-remote', '--profile', profile, '--screenshot', file, `--window-size=${cols * CELL_W * 2},${rows * CELL_H * 2}`, `file://${page}`], { stdio: 'ignore' });
  rmSync(dir, { recursive: true });
}

function toHtml(grid, { ground, tokens }) {
  const rows = grid.map((row) => {
    const cells = row.map((cell) => {
      const style = tokens[cell.k] ?? tokens.fg;
      const bg = cell.bg ? tokens[cell.bg].bg : null;
      // Fonts leave gaps around block glyphs, so whole and half blocks are painted.
      const under = bg ?? 'transparent';
      const block = { '█': style.fg, '▀': `linear-gradient(${style.fg} 50%,${under} 50%)`, '▄': `linear-gradient(${under} 50%,${style.fg} 50%)` }[cell.c];
      if (block) return `<i style="background:${block}"></i>`;
      const css = [`color:${style.fg}`, bg && `background:${bg}`, style.bold && 'font-weight:700', style.underline && 'text-decoration:underline'].filter(Boolean).join(';');
      return `<i style="${css}">${escapeHtml(cell.c)}</i>`;
    });
    return `<div>${cells.join('')}</div>`;
  });
  return `<!doctype html><meta charset="utf-8"><style>
html,body{margin:0;background:${ground}}
body{font:15.5px/${CELL_H}px Menlo,monospace;font-variant-ligatures:none}
div{display:flex;height:${CELL_H}px}
i{font-style:normal;flex:0 0 ${CELL_W}px;width:${CELL_W}px;height:${CELL_H}px;white-space:pre;overflow:visible}
</style>${rows.join('')}`;
}

function screenshot(chrome, html, file, cols, rows) {
  const dir = mkdtempSync(join(tmpdir(), 'cyclops-design-'));
  const page = join(dir, 'screen.html');
  writeFileSync(page, html);
  execFileSync(chrome, ['--headless', '--disable-gpu', '--hide-scrollbars', '--force-device-scale-factor=2', `--window-size=${cols * CELL_W},${rows * CELL_H}`, `--screenshot=${file}`, `file://${page}`], { stdio: 'ignore' });
  rmSync(dir, { recursive: true });
}

const themes = loadTokens();
const browser = process.env.FIREFOX ?? process.env.CHROME;
const capture = process.env.FIREFOX ? firefoxScreenshot : screenshot;
const only = process.env.ONLY?.split(',');

mkdirSync(SCREENS, { recursive: true });
writeFileSync(join(HERE, 'tokens.json'), `${JSON.stringify(themes, null, 2)}\n`);

for (const [name, board, props] of BOARDS) {
  const screen = loadBoard(board, props);
  const grid = drawScreen(screen, themes.dark.tokens);
  const text = grid.map((row) => row.map((cell) => cell.c).join('').trimEnd()).join('\n');
  writeFileSync(join(SCREENS, `${name}.txt`), `${text}\n`);
  const runs = toRuns(grid).map((line) => JSON.stringify(line)).join(',\n');
  writeFileSync(join(SCREENS, `${name}.cells.json`), `{"cols":${screen.cols},"rows":${screen.rows},"lines":[\n${runs}\n]}\n`);
  if (browser && (!only || only.includes(name))) {
    for (const theme of ['dark', 'light']) {
      capture(browser, toHtml(grid, themes[theme]), join(SCREENS, `${name}.${theme}.png`), screen.cols, screen.rows);
    }
  }
}
