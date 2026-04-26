// 새별 마크다운 에디터 (sbmd) — minimal editor frontend wired to the Rust core over
// Tauri IPC. Keeps a plain-text "committed" buffer and a "preedit"
// string; renders them side-by-side in a contenteditable div.

const invoke = window.__TAURI__?.core?.invoke;
if (!invoke) {
  document.body.textContent = 'Tauri IPC bridge not available.';
  throw new Error('missing Tauri IPC');
}

// 릴리즈 빌드에선 브라우저·WebView 네이티브 컨텍스트 메뉴(“Inspect
// Element” 포함)를 전역으로 차단한다. 디버그 빌드(`cargo dev`)에선 개발
// 중 Inspect 접근을 남겨 두기 위해 건드리지 않는다. capture phase 에서
// preventDefault 만 호출하므로, 에디터/탭/파일트리/표 등에서 등록한
// 커스텀 한글 메뉴(openEditorCtxMenu 등)는 버블 단계에서 그대로 동작해
// 원하는 항목만 노출된다. Cmd+Opt+I (macOS) / F12 등의 기본 devtools
// 키보드 단축키는 이 차단과 무관하게 평소대로 작동한다.
(async () => {
  let isDev = false;
  try { isDev = !!(await invoke('is_dev_build')); } catch (_) {}
  if (isDev) return;
  window.addEventListener('contextmenu', (ev) => {
    ev.preventDefault();
  }, true);
})();

const editor = document.getElementById('editor');
const layoutSelect = document.getElementById('layout-select');
const latinSelect = document.getElementById('latin-select');
const langToggle = document.getElementById('lang-toggle');
const langCurrentEl = document.getElementById('lang-current');
const langOtherEl = document.getElementById('lang-other');
const formSelect = document.getElementById('form-select');
const bsModeSelect = document.getElementById('bs-mode-select');
const moachigiToggle = document.getElementById('moachigi-toggle');
const modeLabel = document.getElementById('mode-label');
const abbrToggle = document.getElementById('abbr-toggle');
const fsmStateEl = document.getElementById('fsm-state');
const preeditDebugEl = document.getElementById('preedit-debug');
const lastEventEl = document.getElementById('last-event');
const keyboardMapEl = document.getElementById('keyboard-map');
const helpBtn = document.getElementById('help-btn');
const helpModal = document.getElementById('help-modal');
const helpClose = document.getElementById('help-close');
const helpBackdrop = helpModal.querySelector('.help-backdrop');
const suggestionsEl = document.getElementById('suggestions');
const suggestionsListEl = document.getElementById('suggestions-list');
const mdToggle = document.getElementById('md-toggle');
const mdToolbar = document.getElementById('md-toolbar');
const themeSelect = document.getElementById('theme-select');

/* ─────────── Theme persistence ─────────── */
const THEME_KEY = 'leaf-ime:theme';
// Bright, neutral Material-adjacent light theme — the comfortable default
// for fresh installs. Users can still pick any dark theme from Settings.
const DEFAULT_THEME = 'slate';
const VALID_THEMES = new Set([
  // 다크
  'graphite', 'midnight', 'rosepine', 'dracula', 'tokyonight', 'solarized-dark',
  // 라이트
  'moss', 'sepia', 'latte', 'slate', 'arctic', 'solarized-light', 'github-light',
]);
function applyTheme(name) {
  const t = VALID_THEMES.has(name) ? name : DEFAULT_THEME;
  if (t === 'graphite') document.documentElement.removeAttribute('data-theme');
  else document.documentElement.setAttribute('data-theme', t);
  if (themeSelect && themeSelect.value !== t) themeSelect.value = t;
  try { localStorage.setItem(THEME_KEY, t); } catch (_) {}
  // 상단 '테마' 메뉴는 라디오. 13 개에 개별 set_menu_check 을 뿌리면
  // IPC 순서 경쟁으로 둘 이상 체크된 것처럼 보이는 경우가 생기므로,
  // 원자적인 Rust 커맨드 한 번으로 active 테마만 true, 나머지 theme-*
  // 은 전부 false 로 맞춘다.
  if (typeof invoke === 'function') {
    invoke('set_theme_check_exclusive', { active: t }).catch(() => {});
  }
}
// Apply saved theme as early as possible so the UI doesn't flash the
// fallback. On first run (no saved preference) land on the bright
// default.
try {
  const saved = localStorage.getItem(THEME_KEY);
  applyTheme(saved || DEFAULT_THEME);
} catch (_) {
  applyTheme(DEFAULT_THEME);
}
if (themeSelect) {
  themeSelect.addEventListener('change', () => applyTheme(themeSelect.value));
}

/* ─────────── Editor width persistence ─────────── */
const EDITOR_WIDTH_KEY = 'leaf-ime:editor-width';
function applyEditorWidth(value) {
  const v = value === 'none' ? 'none' : `${parseInt(value, 10) || 760}px`;
  document.documentElement.style.setProperty('--editor-max-width', v);
  try { localStorage.setItem(EDITOR_WIDTH_KEY, value); } catch (_) {}
}
try {
  const savedW = localStorage.getItem(EDITOR_WIDTH_KEY);
  if (savedW) applyEditorWidth(savedW);
} catch (_) {}
document.addEventListener('DOMContentLoaded', () => {
  const sel = document.getElementById('editor-width');
  if (!sel) return;
  const saved = (() => { try { return localStorage.getItem(EDITOR_WIDTH_KEY); } catch { return null; } })();
  if (saved) sel.value = saved;
  sel.addEventListener('change', () => applyEditorWidth(sel.value));
});

/* ─────────── Print ─────────── */
// 인쇄 트리거. WKWebView(macOS) 의 JS `window.print()` 는 네이티브 프린트
// 패널을 못 여는 경우가 있어, Tauri 2 의 WebviewWindow.print() 를 Rust
// 커맨드로 먼저 호출하고 실패 시 JS API 로 폴백한다. IME 의 조합 중인
// preedit 이 섞여 인쇄되지 않게 flush 도 선행한다.
async function doPrint() {
  try { await invoke('flush'); } catch (_) {}
  // 화면에 남아 있던 캐럿 하이라이트를 잠깐 벗겨 내서 인쇄 페이지에
  // markdown syntax marker 가 번져 나오지 않게 한다.
  editor.querySelectorAll('.md-line.has-caret').forEach((el) => el.classList.remove('has-caret'));
  // 스타일 변경이 레이아웃에 반영된 다음 프린트를 띄우도록 한 프레임 대기.
  await new Promise((r) => requestAnimationFrame(() => r()));
  try {
    await invoke('print_webview');
    return;
  } catch (e) {
    // Tauri 커맨드가 비활성화됐거나 해당 플랫폼이 지원하지 않으면 브라우저
    // 네이티브 window.print() 로 넘어간다.
    console.warn('native print failed, falling back to window.print():', e);
  }
  try { window.print(); } catch (_) {}
}
document.addEventListener('DOMContentLoaded', () => {
  const btn = document.getElementById('print-btn');
  if (btn) btn.addEventListener('click', doPrint);
});

/* ─────────── Export (PDF / HTML / Markdown) ─────────── */
// 현재 활성 탭을 선택한 포맷으로 내보낸다. PDF 는 OS 프린트 패널의
// "PDF 로 저장" 경로를 타므로 doPrint() 를 그대로 재사용한다. md 는
// committed 소스 그대로, html 은 renderMarkdownInto 로 빌드한 정적
// 문서(스타일 인라인)를 Rust save 다이얼로그로 넘겨서 기록한다.
async function exportActiveTab(format) {
  commitTabState();
  const t = activeTab();
  if (!t) { logEvent('내보내기: 활성 탭이 없음'); return; }
  // preedit 이 남아 있으면 committed 에는 빠져 있으니 먼저 확정한다.
  try { await invoke('flush'); } catch (_) {}

  if (format === 'pdf') {
    // 유저는 OS 프린트 패널에서 "PDF 로 저장" 을 눌러 파일로 저장한다.
    await doPrint();
    return;
  }

  const baseName = (t.title || '문서').replace(/\.(md|markdown|mdx|html?|pdf)$/i, '');
  if (format === 'md') {
    try {
      const savedPath = await invoke('export_file', {
        format: 'md',
        suggestedName: `${baseName}.md`,
        content: t.committed,
      });
      if (savedPath) logEvent(`내보내기 완료: ${savedPath}`);
    } catch (e) {
      logEvent(`내보내기 실패: ${e && e.message || e}`);
    }
    return;
  }

  if (format === 'html') {
    try {
      const html = await buildExportHtml(t.committed, t.title || baseName);
      const savedPath = await invoke('export_file', {
        format: 'html',
        suggestedName: `${baseName}.html`,
        content: html,
      });
      if (savedPath) logEvent(`내보내기 완료: ${savedPath}`);
    } catch (e) {
      logEvent(`내보내기 실패: ${e && e.message || e}`);
    }
    return;
  }
}

// 커밋된 마크다운 소스를 그대로 살려서 정적 HTML 문서 문자열을 만든다.
// renderMarkdownInto 를 detached div 에 돌려 에디터 DOM 과 똑같이 생긴
// 프리뷰를 얻고, 여기에 highlight.js 를 한 번 돌려 코드블록 하이라이팅을
// 정적으로 박아 넣는다. 스타일은 현재 프런트의 style.css 를 fetch 해서
// <style> 태그로 인라인한다(외부 의존 없이 단일 파일로 열리게).
async function buildExportHtml(sourceMd, title) {
  const container = document.createElement('div');
  container.className = 'editor md-mode';
  renderMarkdownInto(container, sourceMd, '', 0);
  // 에디터 크롬(코드블록 언어 드롭다운 / 테이블 separator 등)과 캐럿
  // 하이라이트 클래스를 제거해 정적 문서에 맞게 정돈.
  container.querySelectorAll('.md-code-lang-select').forEach((el) => el.remove());
  container.querySelectorAll('.md-table-sep').forEach((el) => el.remove());
  container.querySelectorAll('[contenteditable]').forEach((el) => el.removeAttribute('contenteditable'));
  container.querySelectorAll('.has-caret').forEach((el) => el.classList.remove('has-caret'));
  await highlightContainer(container);

  let css = '';
  try {
    const res = await fetch('style.css');
    if (res.ok) css = await res.text();
  } catch (_) {}

  const escapeHtml = (s) => String(s || '').replace(/[<>&"']/g, (c) => (
    { '<': '&lt;', '>': '&gt;', '&': '&amp;', '"': '&quot;', "'": '&#39;' }[c]
  ));
  const safeTitle = escapeHtml(title || '문서');
  const theme = document.documentElement.dataset.theme || '';

  // 에디터로서의 크롬(테두리/섀도/여백 등)은 종이 느낌으로 중화하고,
  // @media print 블록의 색 보정·chrome hide 규칙을 상시 적용해 어느
  // 브라우저로 열든 같은 모양이 되게 한다.
  const overlay = `
html, body { margin: 0; padding: 0; background: var(--md-bg, #ffffff); }
body { padding: 40px 32px; color: var(--md-ink, #1a1a1a); }
.editor {
  max-width: 760px;
  margin: 0 auto;
  border: none !important;
  box-shadow: none !important;
  padding: 0 !important;
  background: transparent !important;
  outline: none !important;
  caret-color: transparent;
}
.md-syn { display: none !important; }
.md-code-lang-select { display: none !important; }
.suggestions, .md-toolbar, header, aside, .files-pane, .tab-bar { display: none !important; }
*, *::before, *::after {
  -webkit-print-color-adjust: exact !important;
  print-color-adjust: exact !important;
}
`;

  return `<!doctype html>
<html lang="ko"${theme ? ` data-theme="${escapeHtml(theme)}"` : ''}>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>${safeTitle}</title>
<style>
${css}
${overlay}
</style>
</head>
<body>
${container.outerHTML}
</body>
</html>`;
}

// highlightCodeBlocks 를 detached 컨테이너에 맞춰 돌려주는 버전.
// 기존 함수는 `editor` 전역에 박혀 있어 export 용 임시 DOM 에는 쓸 수 없다.
async function highlightContainer(root) {
  const codeLines = root.querySelectorAll('.md-code-line[data-lang]:not([data-hl])');
  if (!codeLines.length) return;
  const hljs = await ensureHljs().catch(() => null);
  if (!hljs) return;
  const resolved = new Map();
  for (const line of codeLines) {
    const lang = line.dataset.lang;
    if (lang === 'mermaid') continue;
    if (!resolved.has(lang)) resolved.set(lang, resolveHljsLang(hljs, lang));
  }
  for (const line of codeLines) {
    const lang = line.dataset.lang;
    if (lang === 'mermaid') { line.dataset.hl = '1'; continue; }
    const resolvedLang = resolved.get(lang);
    if (!resolvedLang) { line.dataset.hl = '1'; continue; }
    const src = line.textContent;
    try {
      line.innerHTML = hljs.highlight(src, { language: resolvedLang, ignoreIllegals: true }).value;
    } catch (_) {}
    line.dataset.hl = '1';
  }
}

/* ─────────── Native application menu wiring ─────────── */
// The menu lives in the host OS (macOS menu bar / Windows or Linux
// window menu), populated from Rust in `build_app_menu`. Tauri emits a
// `menu-action` event with the item's id; we dispatch to the same
// handlers that used to back the in-window menubar.
async function openMarkdownFile() {
  try {
    const picked = await invoke('pick_markdown_file');
    if (!picked) return;
    const existing = tabs.find((t) => t.path === picked.path);
    if (existing) {
      commitTabState();
      loadTabState(existing.id);
      return;
    }
    addTab({
      title: picked.name,
      path: picked.path,
      committed: picked.content,
      savedCommitted: picked.content,
    });
    if (!markdownMode) {
      markdownMode = true;
      if (mdToggle) mdToggle.checked = true;
      try { localStorage.setItem(MD_KEY, '1'); } catch (_) {}
      render();
    }
  } catch (e) {
    logEvent(`파일 열기 실패: ${e}`);
  }
}

async function handleMenuAction(action) {
  logEvent(`menu: ${action}`);
  try {
    switch (action) {
      case 'new-tab': addTab({}); editor.focus(); break;
      case 'open-file': await openMarkdownFile(); break;
      case 'open-dir': await openDirectory(); break;
      case 'save': await saveActiveTab(); break;
      case 'close-tab': if (activeTabId != null) await closeTab(activeTabId); break;
      case 'print': doPrint(); break;
      case 'export-pdf': await exportActiveTab('pdf'); break;
      case 'export-html': await exportActiveTab('html'); break;
      case 'export-md': await exportActiveTab('md'); break;
      // ── 본문(Paragraph) 메뉴 ──
      case 'para-h1': await mdReplaceLinePrefix(mdToggleHeading(1)); break;
      case 'para-h2': await mdReplaceLinePrefix(mdToggleHeading(2)); break;
      case 'para-h3': await mdReplaceLinePrefix(mdToggleHeading(3)); break;
      case 'para-h4': await mdReplaceLinePrefix(mdToggleHeading(4)); break;
      case 'para-h5': await mdReplaceLinePrefix(mdToggleHeading(5)); break;
      case 'para-h6': await mdReplaceLinePrefix(mdToggleHeading(6)); break;
      case 'para-body': await mdReplaceLinePrefix(mdToggleHeading(0)); break;
      case 'para-heading-promote': await mdReplaceLinePrefix(mdPromoteHeading()); break;
      case 'para-heading-demote': await mdReplaceLinePrefix(mdDemoteHeading()); break;
      case 'para-table-insert': {
        const size = await pickTableSize();
        if (size) await mdInsertTable(size.rows, size.cols);
        break;
      }
      case 'para-math-block': await mdInsertMathBlock(); break;
      case 'para-code-fence': await mdInsertCodeFence(); break;
      case 'para-code-trim': await mdTrimCodeBlockAtCaret(); break;
      case 'para-alert-note': await mdInsertAdmonition('NOTE'); break;
      case 'para-alert-tip': await mdInsertAdmonition('TIP'); break;
      case 'para-alert-important': await mdInsertAdmonition('IMPORTANT'); break;
      case 'para-alert-warning': await mdInsertAdmonition('WARNING'); break;
      case 'para-alert-caution': await mdInsertAdmonition('CAUTION'); break;
      case 'para-quote': await mdReplaceLinePrefix(mdToggleQuote()); break;
      case 'para-ol': await mdReplaceLinePrefix(mdToggleOrdered()); break;
      case 'para-ul': await mdReplaceLinePrefix(mdToggleBullet()); break;
      case 'para-task': await mdReplaceLinePrefix(mdToggleTask()); break;
      case 'para-task-check': await mdSetTaskState('checked'); break;
      case 'para-task-uncheck': await mdSetTaskState('unchecked'); break;
      case 'para-task-toggle': await mdSetTaskState('toggle'); break;
      case 'para-indent-in': await mdAdjustIndent(1); break;
      case 'para-indent-out': await mdAdjustIndent(-1); break;
      case 'para-indent-reset': await mdAdjustIndent(0); break;
      case 'para-insert-before': await mdInsertParagraphBefore(); break;
      case 'para-insert-after': await mdInsertParagraphAfter(); break;
      case 'para-link': await mdInsertLink(); break;
      case 'para-footnote': await mdInsertFootnote(); break;
      case 'para-hr': await mdInsertHr(); break;
      case 'para-toc': await mdInsertToc(); break;
      case 'para-yaml': await mdInsertYamlFrontMatter(); break;
      case 'settings': openSettings(); break;
      case 'appearance': openAppearance(); break;
      case 'help': openHelp(); break;
      case 'toggle-stylebar': toggleStyleBar(); break;
      case 'toggle-statusbar': toggleStatusBar(); break;
      case 'toggle-source': toggleSourceView(); break;
      case 'sidebar-outline': toggleOutlineView(); break;
      case 'toggle-keymap-panel': toggleKeymapPanel(); break;
      case 'toggle-abbrs-panel': toggleAbbrsPanel(); break;
      case 'toggle-zoom-allow': toggleZoomAllow(); break;
      case 'zoom-reset': zoomReset(); break;
      case 'zoom-in': zoomIn(); break;
      case 'zoom-out': zoomOut(); break;
      case 'toggle-always-on-top': await toggleAlwaysOnTop(); break;
      case 'toggle-fullscreen': await toggleFullscreen(); break;
      case 'reset-settings': await resetAllSettings(); break;
      case 'doc-moachigi': openBuiltinDoc('sebeolsik-final-moachigi'); break;
      case 'doc-markdown': openBuiltinDoc('markdown-editing'); break;
      case 'doc-autocomplete': openBuiltinDoc('autocomplete-usage'); break;
      case 'doc-files': openBuiltinDoc('files-tabs'); break;
      case 'doc-shortcuts': openBuiltinDoc('shortcuts'); break;
      case 'about': openAbout(); break;
      case 'check-update': await promptForUpdateIfAny({ silentIfNone: false }); break;
      default:
        if (typeof action === 'string' && action.startsWith('theme-')) {
          applyTheme(action.slice('theme-'.length));
        }
        break;
    }
    // 네이티브 메뉴에서 항목을 선택하면 focus 가 메뉴바에 있다가 반환
    // 되는데, 문단 편집 액션은 곧바로 타이핑을 이어가는 흐름이라 캐럿은
    // 있어도 focus 가 돌아오지 않으면 타자가 먹지 않는다. `render()` 만
    // 으로는 focus 복원이 보장되지 않아 명시적으로 한 번 더 잡아준다.
    if (action && action.startsWith('para-')) {
      try { editor.focus(); } catch {}
    }
  } catch (e) {
    logEvent(`menu '${action}' 실패: ${e && e.message || e}`);
  }
}

/* ─────────── Reset settings ─────────── */
async function resetAllSettings() {
  const answer = await askConfirm({
    title: '설정을 모두 초기화할까요?',
    body:
      '모든 사용자 설정 — 테마 · 한글/영문 자판 · 입력 모드 · 출력 형식 · 모아치기 · 사이드바 · 스타일 바 · 상태표시줄 · 본문 폰트 · 타이포 자동교정 · 줌 · 항상 위에 · 찾기/바꾸기 기록 · 핀 고정 · 마지막으로 연 디렉터리 — 이 기본값으로 돌아갑니다.\n\n' +
      '예외 — 다음 항목은 초기화되지 않고 그대로 보존됩니다:\n' +
      '• 열려 있던 탭과 각 탭의 내용·커서 위치 (활성 탭 포함)\n' +
      '• 자동완성 사용자 사전 abbreviations.toml\n' +
      '• 학습된 n-gram 자동완성 사전 learned_ngrams.toml\n' +
      '• 디스크에 저장된 마크다운 파일과 폴더 구조\n\n' +
      '참고:\n' +
      '• 에디터 폭은 선언된 기본값이 아닌 권장값 880px 로 다시 설정됩니다.\n' +
      '• 자동완성 사용자 사전은 다시 ON 상태로 복원됩니다.\n' +
      '• 초기화 직후 앱이 자동으로 새로고침되므로 활성 탭의 미저장 편집(최대 약 0.3초 분)은 누락될 수 있습니다 — 먼저 저장해 두세요.',
    saveLabel: '초기화',
    discardLabel: '',
    cancelLabel: '취소',
    showSave: true,
  });
  if (answer !== 'save') return;
  try {
    const keys = Object.keys(localStorage).filter((k) => k.startsWith('leaf-ime:'));
    for (const k of keys) {
      if (k === 'leaf-ime:tabs') continue; // keep open tabs
      localStorage.removeItem(k);
    }
    // Post-reset preferences: editor width 넓게 (880px) — the comfortable
    // default users expressed a preference for. Everything else falls
    // back to its own declared default on next load.
    localStorage.setItem(EDITOR_WIDTH_KEY, '880');
  } catch (_) {}
  await invoke('reset_settings').catch(() => null);
  logEvent('설정 초기화 — 새로고침');
  window.location.reload();
}

/* ─────────── Abbreviation dictionary ─────────── */
async function loadAbbrDict() {
  try {
    const count = await invoke('pick_and_load_abbr_dict');
    if (count === null || count === undefined) { logEvent('사전 불러오기 취소됨'); return; }
    logEvent(`자동완성 사전 로드 — ${count}개 항목`);
    await renderAbbrList();
  } catch (e) {
    logEvent(`사전 로드 실패: ${e}`);
  }
}
async function learnFromDir() {
  try {
    statusMessage?.('학습 폴더 스캔 중…');
    // rootDir 은 사이드바 트리로 마지막에 연 폴더. 학습 다이얼로그가
    // 같은 자리에서 열리도록 백엔드로 전달한다 (없으면 Rust 쪽에서
    // 자동으로 무시하고 OS 기본 경로를 쓴다).
    const startDir = (typeof rootDir === 'string' && rootDir) ? rootDir : null;
    const report = await invoke('scan_and_build_ngram_dict', { startDir });
    if (!report) { logEvent('학습 취소됨'); statusMessage?.(''); return; }
    const { count, files, tokens, dir } = report;
    logEvent(`자동완성 학습 완료 — ${count}개 약어 등록 (파일 ${files}개 · 어절 ${tokens}개 · ${dir})`);
    statusMessage?.(`자동완성 학습 완료 — ${count}개 약어 등록`);
    await askConfirm({
      title: '자동완성 학습 완료',
      body: `총 ${count.toLocaleString()}개의 약어가 등록되었습니다.\n` +
            `스캔한 파일: ${files.toLocaleString()}개\n` +
            `어절 수: ${tokens.toLocaleString()}개\n` +
            `폴더: ${dir}\n\n` +
            '학습 결과는 앱 설정 폴더의 learned_ngrams.toml 에 저장되어 다음 실행에도 유지됩니다.',
      saveLabel: '확인',
      discardLabel: '',
      cancelLabel: '',
      showSave: true,
    });
    await renderAbbrList();
  } catch (e) {
    logEvent(`학습 실패: ${e && e.message || e}`);
    statusMessage?.(`학습 실패: ${e}`);
  }
}
async function resetAbbrDict() {
  try {
    const count = await invoke('reset_abbr_dict');
    logEvent(`자동완성 사전 기본값 복원 (사용자 사전 ${count}개)`);
    await renderAbbrList();
  } catch (e) {
    logEvent(`복원 실패: ${e}`);
  }
}

/* ─────────── Built-in docs (open as a new markdown tab) ─────────── */
const BUILTIN_DOCS = {
  'sebeolsik-final-moachigi': {
    title: '세벌식 최종 · 모아치기.md',
    body: `# 세벌식 최종 · 모아치기 규칙

공병우 박사가 1991년에 완성한 **세벌식 최종(3-91)** 자판을 새별 마크다운 에디터에서 모아치기(순서 무관 조합) 로 사용할 때의 결합 규칙을 정리한 문서입니다.

---

## 1. 세벌식의 구조

세벌식은 한글 음절을 이루는 **세 자리**를 키보드에서 서로 다른 영역에 배치합니다.

- **초성** — 첫소리(자음). 키보드 **오른쪽** 영역
- **중성** — 가운뎃소리(모음). 키보드 **가운데** 영역
- **종성** — 끝소리(받침, 자음). 키보드 **왼쪽** 영역 + 숫자열

이렇게 역할별로 키가 **단일 역할** 로 고정되어 있어 초성·중성·종성이 섞일 일이 없습니다. 덕분에 "어느 자리의 자모가 먼저 들어왔는가" 와 무관하게 한 음절을 맞출 수 있습니다.

---

## 2. 키 배치

### 초성 (오른쪽)

| 키 | 자모 | 키 | 자모 |
|:-:|:--:|:-:|:--:|
| \`j\` | ㅇ | \`k\` | ㄱ |
| \`l\` | ㅈ | \`;\` | ㅂ |
| \`'\` | ㅌ | \`h\` | ㄴ |
| \`u\` | ㄷ | \`i\` | ㅁ |
| \`y\` | ㄹ | \`o\` | ㅊ |
| \`p\` | ㅍ | \`n\` | ㅅ |
| \`m\` | ㅎ | \`0\` | ㅋ |

### 중성 (가운데)

| 키 | 자모 | 키 | 자모 |
|:-:|:--:|:-:|:--:|
| \`f\` | ㅏ | \`d\` | ㅣ |
| \`g\` | ㅡ | \`c\` | ㅔ |
| \`v\` / \`/\` | ㅗ | \`b\` | ㅜ |
| \`e\` | ㅕ | \`r\` | ㅐ |
| \`t\` | ㅓ | \`6\` | ㅑ |
| \`4\` | ㅛ | \`5\` | ㅠ |
| \`7\` | ㅖ | \`8\` | ㅢ |
| \`9\` | ㅜ | Shift+G | ㅒ |

### 종성 (왼쪽 · 숫자열)

| 키 | 자모 | 키 | 자모 |
|:-:|:--:|:-:|:--:|
| \`q\` | ㅅ | \`w\` | ㄹ |
| \`a\` | ㅇ | \`s\` | ㄴ |
| \`z\` | ㅁ | \`x\` | ㄱ |
| \`1\` | ㅎ | \`2\` | ㅆ |
| \`3\` | ㅂ | | |

### Shift 레이어 — 직접 겹받침

| 키 | 받침 | 설명 |
|:-:|:--:|:--|
| Shift+1 | ㄲ | |
| Shift+2 | ㄺ | ㄹ+ㄱ |
| Shift+3 | ㅈ (받침) | |
| Shift+4 | ㄿ | ㄹ+ㅍ |
| Shift+5 | ㄾ | ㄹ+ㅌ |
| Shift+A | ㄷ (받침) | |
| Shift+C | ㅋ (받침) | |
| Shift+D | ㄼ | ㄹ+ㅂ |
| Shift+E | ㄵ | ㄴ+ㅈ |
| Shift+F | ㄻ | ㄹ+ㅁ |
| Shift+Q | ㅍ (받침) | |
| Shift+R | ㅀ | ㄹ+ㅎ |
| Shift+S | ㄶ | ㄴ+ㅎ |
| Shift+T | ㄽ | ㄹ+ㅅ |
| Shift+V | ㄳ | ㄱ+ㅅ |
| Shift+W | ㅌ (받침) | |
| Shift+X | ㅄ | ㅂ+ㅅ |
| Shift+Z | ㅊ (받침) | |

> 숫자 자체가 필요할 때는 오른손 Shift 열을 쓰세요: Shift+Y~P = 5·6·7·8·9, Shift+H~L = 0·1·2·3.

---

## 3. 모아치기 결합 규칙

새별 마크다운 에디터의 모아치기 모드는 다음 원칙으로 음절을 합칩니다.

### 3-1. 순서 무관

같은 음절 안에서는 **초성·중성·종성을 어느 순서로 눌러도** 한 음절로 합쳐집니다.

- \`j f s\` (초·중·종) → **안**
- \`s f j\` (종·중·초, 역순) → **안**
- \`f s j\` (중·종·초, 뒤섞기) → **안**

### 3-2. 한 슬롯당 하나

각 슬롯(초·중·종)은 **한 번만** 채워집니다. 같은 슬롯을 두 번째로 채우면:

- **초성 중복** — 처음 음절을 확정(커밋)하고 두 번째 초성이 새 음절의 시작이 됩니다.
  - \`j j\` → **ㅇ**(확정) + 새 음절 초성 **ㅇ**
  - 단, 쌍자음이 되는 경우(ㄱ+ㄱ=ㄲ 등)에는 중성이 아직 없는 동안만 자리에서 합쳐집니다 — 세벌식에서는 실제로 거의 쓰이지 않음.

### 3-3. 겹받침 형성

종성 슬롯에 이미 단받침이 있고, 새로 들어온 받침과 **결합 규칙이 있으면** 겹받침으로 합쳐집니다.

- \`k f w x\` (ㄱ+ㅏ+ㄹ+ㄱ) → **갉** (ᆯ+ᆨ = ᆰ)
- \`k f x q\` (ㄱ+ㅏ+ㄱ+ㅅ) → **갃** (ᆨ+ᆺ = ᆪ)
- \`k f 3 q\` (ㄱ+ㅏ+ㅂ+ㅅ) → **값** (ᆸ+ᆺ = ᆹ)

### 3-4. 직접 겹받침 키

Shift 레이어의 겹받침 키는 세 가지 방식 모두로 입력할 수 있습니다.

1. **컴포넌트 합성** — 단받침 두 개를 차례로: \`ᆯ + ᆨ\` → \`ᆰ\`
2. **직접 키** — Shift+2 한 번: \`ᆰ\`
3. **단받침 → 직접 키** — \`ᆯ\` 먼저 치고 \`Shift+2\`(ᆰ) 를 마저 침: 기존 \`ᆯ\` 을 흡수해 \`ᆰ\` 으로 승격

세 경로 모두 동일한 결과를 냅니다.

### 3-5. 음절 경계 ★

> 한 음절의 **초성·중성·종성이 모두 채워지면** 그 음절은 "완성" 으로 간주되어, 다음 입력은 새 음절의 시작이 됩니다. 이전 음절에 **영향을 미치지 않습니다**.

- \`j f s\` → **안** (ᄋ+ᅡ+ᆫ, 완성)
- 이어서 \`l f\` → **안 + 자** (새 음절 ㅈ+ㅏ)
  - ~~앉 (ᆫ+ᆽ 겹받침) 은 아님~~ · 완성된 음절은 건드리지 않음

마찬가지로 완성된 음절 뒤에 오는 모음은 새 음절의 중성으로 들어갑니다.

- \`k v x\` → **곡**
- 이어서 \`f\` → **곡 + 아** (새 음절 ㅏ)
  - ~~곽 (ᅩ+ᅡ = ᅪ) 은 아님~~

### 3-6. 미완성 음절에서의 합성 유지

음절이 아직 미완성 (초성·중성·종성 중 하나 이상이 비어 있음) 이면 기존 규칙대로 **같은 슬롯 안에서 합성** 이 일어납니다.

- 초성 없이 종성만 먼저: \`w d x\` (ㄹ+ㅣ+ㄱ 순) → \`ᆯ\` 쌓고 \`ᅵ\` 넣고 \`ᆨ\` 도착 시 ᆯ+ᆨ=ᆰ → **릵** 의 초성 없는 자모
- 모음 합성: \`v f\` (ㅗ+ㅏ) → **ᅪ** (초성이 아직 없으니 같은 음절 안)

---

## 4. 자주 쓰는 예시

| 원하는 글자 | 입력 | 설명 |
|:-:|:--|:--|
| **안녕** | \`j f s h e a\` | 안: ᄋ+ᅡ+ᆫ · 녕: ᄂ+ᅧ+ᆼ |
| **학교** | \`m f x · k 4\` | 학: ᄒ+ᅡ+ᆨ · 교: ᄀ+ᅭ |
| **먹다** | \`i t x · u f\` | 먹: ᄆ+ᅥ+ᆨ · 다: ᄃ+ᅡ |
| **값** | \`k f 3 q\` | 겹받침 ㅄ 을 ㅂ+ㅅ 으로 |
| **갔다** | \`k f 2 · u f\` | ㅆ (받침) 은 Digit2 |
| **앉아** | \`j f Shift+E · j f\` | Shift+E 로 ᆬ 직접 입력 |
| **과자** | \`k v f · l f\` | 과: ᄀ+ᅪ(ᅩ+ᅡ) · 자: ᄌ+ᅡ |

---

## 5. 입력 모드 팁

- **Caps Lock** 또는 **Shift + Space** — 한글 ↔ 영문 전환
- **Space** / **Enter** — 조합 중인 음절을 확정
- **Esc** — 조합 취소 (현재 음절 버림)
- **Backspace** — 자모 단위로 하나씩 제거 (겹받침 → 단받침 → 초성 → 없음)

하단 상태 바의 **세벌식 최종 · 모아치기** 배지를 클릭하면 자판/조합 방식을 즉시 전환할 수 있습니다.
`,
  },

  'markdown-editing': {
    title: '마크다운 안내.md',
    body: `# 마크다운 안내

새별은 **항상-렌더링** 방식으로 마크다운을 보여 줍니다. \`**\` · \`#\` · \`|\` 같은 문법 기호는 커서가 그 줄에 있어도 보이지 않고, 최종 결과에 가까운 모습으로 편집합니다.

---

## 1. 블록

### 제목
- 줄 맨 앞에 \`# \` ~ \`###### \` → 해당 레벨의 제목.
- \`⌘ 1\` … \`⌘ 6\` 토글, \`⌘ 0\` 으로 본문 해제.
- \`⌘ =\` / \`⌘ -\` — 제목 한 단계 올리기·내리기.

### 목록
- \`- \` · \`* \` — 점 목록, \`1. \` — 번호 목록.
- **Enter** — 같은 종류의 다음 항목 자동 이어쓰기. 빈 항목에서 다시 Enter 치면 목록 탈출.
- **Tab / ⇧Tab** — 들여쓰기·내어쓰기.
- \`- [ ] \` · \`- [x] \` — 할 일 목록. 렌더된 체크박스 **클릭** 으로 전환.
- 메뉴 단축키: \`⌥⌘ O\`(번호) · \`⌥⌘ U\`(점) · \`⌥⌘ X\`(할 일).

### 인용 · 코드 · 수식 · 구분선
- \`> \` 로 시작하면 인용. \`⌥⌘ Q\` 로 토글.
- \`\`\`\`\` 세 개로 감싼 코드 블록. \`⌥⌘ C\` 로 삽입.
- \`$$…$$\` 수식 블록. \`⌥⌘ B\` 로 삽입. 한 줄짜리 \`$$C = 1 - L/N$$\` 도 자동 렌더.
- \`⌥⌘ -\` — 구분선.

### 표
- 두 번째 줄이 \`|:---:|---:|\` 형태의 구분선이면 표로 렌더. 구분선은 감춰지고 얇은 밑줄로만 표현됩니다.

### 강조 상자 (GFM Alert)
- \`> [!NOTE]\`, \`[!TIP]\`, \`[!IMPORTANT]\`, \`[!WARNING]\`, \`[!CAUTION]\` — 색이 다른 다섯 가지 강조 상자.

---

## 2. 인라인 서식

| 단축키 | 동작 |
|:--|:--|
| \`⌘ B\` | **굵게** |
| \`⌘ I\` | *기울임* |
| \`⌘ E\` | 인라인 코드 |
| \`⌘ ⇧ X\` | ~~취소선~~ |
| \`⌘ K\` | 링크 |

선택 영역이 있으면 그 범위를 감싸고, 없으면 삽입 후 커서를 가운데에 놓습니다.

---

## 3. 이미지

\`![대체텍스트](경로)\` 를 쓰면 본문 아래에 미리보기가 붙습니다. 이미지 주변에서 방향키를 누르면 이미지가 먼저 선택(파란 외곽)되고, 다시 누르면 커서가 그 위/아래 줄로 넘어갑니다.

---

## 4. 찾기·바꾸기

- \`⌘ F\` — 에디터 우상단에 찾기·바꾸기 바 등장.
- Enter/⇧Enter 로 다음/이전, ⇄ 버튼으로 바꾸기 섹션 펼침.
- 일치 위치가 에디터에 실시간 하이라이트 (현재 일치는 짙은 오렌지).

---

## 5. 선택 영역 도구 모음

두 글자 이상 선택하면 **플로팅 툴바**가 뜹니다. 굵게 / 기울임 / 코드 / 링크 / 취소선 — 위 단축키와 같은 동작.

---

## 6. 인쇄·내보내기

- \`⌘ P\` — **인쇄**. 문법 기호가 모두 숨겨진 A4 스타일 출력.
- **파일 → 내보내기** — PDF · HTML · Markdown 중 선택.

---

> **항상 렌더링**: 커서가 어느 줄에 있어도 \`**\`, \`#\`, 표 구분선 같은 문법 기호는 보이지 않습니다. 완성된 문서 그대로 편집하는 것이 새별의 기본 철학입니다.
`,
  },

  'autocomplete-usage': {
    title: '자동완성 사용법.md',
    body: `# 자동완성 사용법

새별 마크다운 에디터의 자동완성은 **초성 줄임말** 을 치면 정해진 긴 표현으로 확장하는 기능입니다.

> **초기 버전 안내** — 안정성을 위해 현재 버전은 **초성(자음) 연타** 트리거만 제공합니다. 음절·어미 기반 자동완성은 비활성화되어 있어, 의도치 않은 확장이 생기지 않습니다.

---

## 1. 동작 흐름

1. 초성 자음을 연속으로 치면(예: \`ㄱㅅ\`) 커서 아래에 후보 팝업이 뜹니다.
2. \`↑\` \`↓\` — 후보 이동, \`Tab\` — 수락, \`Esc\` — 닫기.
3. 수락하면 초성 꼬리가 확장 결과로 치환됩니다.

\`Enter\` 는 줄바꿈으로만 동작하고 후보를 수락하지 않습니다.

---

## 2. 기본 제공 줄임말 (일부)

| 초성 | 확장 결과 |
|:--|:--|
| \`ㄱㅅ\` | 감사합니다. |
| \`ㅇㄴㅎㅅㅇ\` | 안녕하세요. |
| \`ㅅㄱ\` | 수고하셨습니다. |
| \`ㅈㅅㅎㄴㄷ\` | 죄송합니다. |
| \`ㅊㅋ\` | 축하합니다. |
| \`ㅇㅋ\` | 알겠습니다. |
| \`ㄱㅅ\` · \`ㄱㄷ\` | 검토 부탁드립니다. / 고생하셨습니다. |
| \`ㅎㅇ\` | 환영합니다. |
| \`ㅈㅎ\` | 잘 부탁드립니다. |
| \`ㄴㅎㅇㄱㅎㅅㅅㄷ\` → 짧게 \`ㅈㅎㅅ\` | 좋은 하루 되세요. |

---

## 3. 후보가 뜨지 않는 경우

- **빈 줄** 또는 **줄 첫 글자** — 새 단어 시작으로 간주해 생략.
- 바로 이전 글자가 **공백/개행/탭** — 단어 경계 직후라 생략.
- 초성 꼬리가 비어 있을 때.

---

## 4. 팁

- 팝업이 열린 상태에서 \`←\` \`→\` 를 누르면 팝업이 닫히고 커서만 좌우로 움직입니다.
- \`Backspace\` / \`Delete\` 도 팝업을 닫으며, 남은 꼬리 기준으로 다시 검색이 시작됩니다.
- 팝업을 의도적으로 닫은 뒤에는 **새로 글자를 입력할 때까지** 다시 열리지 않아 타이핑을 방해하지 않습니다.
`,
  },

  'files-tabs': {
    title: '파일·탭 안내.md',
    body: `# 파일·탭 안내

새별은 **탭 편집기 + 좌측 사이드바**로 여러 파일을 동시에 다룹니다.

---

## 1. 파일·폴더 열기

| 단축키 | 동작 |
|:--|:--|
| \`⌘ O\` | 폴더 열기 — 사이드바 파일 트리의 루트가 됨 |
| \`⌘ ⇧ O\` | 파일 하나 열기 |
| \`⌘ T\` | 새 탭 |
| \`⌘ S\` | 저장 (경로가 없으면 "다른 이름으로 저장") |
| \`⌘ W\` | 현재 탭 닫기 |
| \`⌘ P\` | 인쇄 |

> 폴더를 열면 사이드바가 자동으로 **파일 트리** 로 전환됩니다.

---

## 2. 탭 우클릭

- **이 파일 위치로** *(저장된 파일만)* — 사이드바 트리를 해당 파일이 들어 있는 폴더로 전환 · 선택.
- 이 탭 · 왼쪽 / 오른쪽 / 다른 / 모든 탭 닫기.

저장되지 않은 변경이 있는 탭을 닫으려 하면 "저장하고 닫을까요?" 확인 창이 뜹니다.

---

## 3. 파일 트리

- 폴더 클릭 — 펼치기/접기.
- 파일 클릭 — 새 탭으로 열기. 이미 열려 있으면 그 탭으로 이동.
- 방향키 ↑/↓ — 선택 이동 (트리에 포커스).
- **Enter** — 선택 항목 이름 바꾸기.
- \`⌘ Backspace\` / \`⌘ Delete\` — 선택 항목 삭제 (확인 창).

### 우클릭 메뉴
- 파일·폴더: 이름 바꾸기 · 복제 · **고정(★)** · 이 폴더로 탐색 · Finder에서 보기 · 경로 복사 · 삭제.
- 빈 영역: 새 파일 · 새 폴더.
- **고정** 된 항목은 목록 상단에 ★ 표시로 모입니다.

---

## 4. 사이드바

- 트리 헤더의 접기 버튼으로 사이드바를 **접기/펼치기**.
- **보기 → 개요 보기** 를 체크하면 사이드바가 **개요** 로 전환되고, 해제하면 다시 파일 트리로 돌아옵니다 (기본).

---

## 5. 세션 복원

앱을 다시 열면 마지막에 열려 있던 폴더와 탭들이 자동 복원됩니다. 저장되지 않은 탭 내용도 로컬에 보존됩니다.
`,
  },

  'shortcuts': {
    title: '단축키 모음.md',
    body: `# 단축키 모음

macOS 에서는 \`⌘(Cmd)\`, Windows/Linux 에서는 \`Ctrl\`. 아래 표는 편의상 \`⌘\` 로 표기합니다.

---

## 파일 · 탭

| 단축키 | 동작 |
|:--|:--|
| \`⌘ T\` | 새 탭 |
| \`⌘ O\` | 폴더 열기 |
| \`⌘ ⇧ O\` | 파일 열기 |
| \`⌘ S\` | 저장 |
| \`⌘ W\` | 탭 닫기 |
| \`⌘ P\` | 인쇄 |

## 편집

| 단축키 | 동작 |
|:--|:--|
| \`⌘ Z\` / \`⌘ ⇧ Z\` | 실행 취소 / 다시 실행 |
| \`⌘ X\` / \`⌘ C\` / \`⌘ V\` | 잘라내기 / 복사 / 붙여넣기 |
| \`⌘ A\` | 모두 선택 |
| \`⌘ F\` | 찾기·바꾸기 바 열기 |
| \`⌘ G\` / \`⌘ ⇧ G\` | 다음 / 이전 일치로 이동 |

## 마크다운 인라인

| 단축키 | 동작 |
|:--|:--|
| \`⌘ B\` | 굵게 |
| \`⌘ I\` | 기울임 |
| \`⌘ E\` | 인라인 코드 |
| \`⌘ K\` | 링크 |
| \`⌘ ⇧ X\` | 취소선 |

## 마크다운 블록

| 단축키 | 동작 |
|:--|:--|
| \`⌘ 0\` | 본문 (제목 해제) |
| \`⌘ 1\` … \`⌘ 6\` | 제목 1 … 6 |
| \`⌘ =\` / \`⌘ -\` | 제목 올리기 / 내리기 |
| \`⌥ ⌘ O\` / \`⌥ ⌘ U\` | 번호 목록 / 점 목록 |
| \`⌥ ⌘ X\` | 할 일 목록 |
| \`⌥ ⌘ Q\` | 인용 |
| \`⌥ ⌘ C\` | 코드 블록 |
| \`⌥ ⌘ B\` | 수식 |
| \`⌥ ⌘ L\` / \`⌥ ⌘ R\` | 링크 / 각주 |
| \`⌥ ⌘ -\` | 구분선 |
| \`Tab\` / \`⇧ Tab\` | 목록 들여쓰기 / 내어쓰기 |
| \`Enter\` | 목록 항목 자동 이어쓰기 |

## 보기

| 단축키 | 동작 |
|:--|:--|
| \`⌘ /\` | 원문 보기 |
| \`⌃ ⌘ 1\` | 개요 보기 |
| \`⌘ ⇧ 0\` | 100% 크기 |
| \`⌘ ⇧ =\` / \`⌘ ⇧ -\` | 확대 / 축소 |
| \`⌃ ⌘ F\` | 전체 화면 |

## 설정 · 도움말

| 단축키 | 동작 |
|:--|:--|
| \`⌘ ,\` | 설정 |
| \`⌘ ⇧ ,\` | 모양 설정 |
| \`F1\` | 도움말 |

## 자동완성 팝업

| 단축키 | 동작 |
|:--|:--|
| \`↑\` / \`↓\` | 후보 이동 |
| \`Tab\` | 수락 |
| \`Esc\` | 닫기 |

## 한글 입력

| 단축키 | 동작 |
|:--|:--|
| \`Caps Lock\` / \`⇧ Space\` | 한 ↔ 영 전환 |
| \`Space\` / \`Enter\` | 조합 중 음절 확정 |
| \`Esc\` | 조합 취소 |
| \`Backspace\` | 자모 단위 되돌리기 |

## 마우스

- \`⌘ + 휠\` — 화면 확대/축소
`,
  },
};

function openBuiltinDoc(id) {
  const doc = BUILTIN_DOCS[id];
  if (!doc) return;
  // If the doc is already open as a tab, just focus it.
  const existing = tabs.find((t) => t.title === doc.title && !t.path);
  if (existing) {
    commitTabState();
    loadTabState(existing.id);
    return;
  }
  addTab({
    title: doc.title,
    committed: doc.body,
    savedCommitted: doc.body, // start un-dirty
  });
  if (!markdownMode) {
    markdownMode = true;
    if (mdToggle) mdToggle.checked = true;
    try { localStorage.setItem(MD_KEY, '1'); } catch (_) {}
    render();
  }
  editor.focus();
}

/* ─────────── Appearance modal ─────────── */
function openAppearance() {
  const m = document.getElementById('appearance-modal');
  if (!m) return;
  m.classList.remove('hidden');
}
function closeAppearance() {
  const m = document.getElementById('appearance-modal');
  if (!m) return;
  m.classList.add('hidden');
  editor.focus();
}

const FONT_STACKS = {
  'system':         '-apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif',
  'sans':           '"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans KR", sans-serif',
  'serif':          '"Iowan Old Style", "Charter", "Noto Serif KR", Georgia, serif',
  'mono':           'ui-monospace, "SF Mono", "JetBrains Mono", Menlo, monospace',
  'nanum-gothic':   '"나눔고딕", "Nanum Gothic", -apple-system, sans-serif',
  'nanum-myeongjo': '"나눔명조", "Nanum Myeongjo", Georgia, serif',
  'noto-sans-kr':   '"Noto Sans KR", -apple-system, sans-serif',
  'noto-serif-kr':  '"Noto Serif KR", "Iowan Old Style", serif',
  'bm-dohyeon':     '"BM Dohyeon", "배민도현체", -apple-system, sans-serif',
};

const AP_DEFAULTS = {
  font: 'serif',
  scale: '1.00',
  lineHeight: '1.80',
  imageSize: 'shrink',
  fsBase: 13, fsRight: 12, fsSidebar: 12, fsToolbar: 13, fsStatus: 12,
};

const AP_KEY = 'leaf-ime:appearance';

function loadAppearance() {
  try {
    const raw = localStorage.getItem(AP_KEY);
    if (!raw) return { ...AP_DEFAULTS };
    return { ...AP_DEFAULTS, ...JSON.parse(raw) };
  } catch {
    return { ...AP_DEFAULTS };
  }
}
function saveAppearance(a) {
  try { localStorage.setItem(AP_KEY, JSON.stringify(a)); } catch (_) {}
}
function applyAppearance(a) {
  const root = document.documentElement;
  root.style.setProperty('--md-font-family', FONT_STACKS[a.font] || FONT_STACKS.serif);
  root.style.setProperty('--ui-scale', a.scale);
  root.style.setProperty('--md-line-height', a.lineHeight);
  root.style.setProperty('--fs-app', a.fsBase + 'px');
  root.style.setProperty('--fs-right', a.fsRight + 'px');
  root.style.setProperty('--fs-sidebar', a.fsSidebar + 'px');
  root.style.setProperty('--fs-toolbar', a.fsToolbar + 'px');
  root.style.setProperty('--fs-status', a.fsStatus + 'px');
  root.dataset.imageSize = a.imageSize;
  root.style.setProperty(
    'font-size',
    `calc(${a.fsBase}px * ${a.scale})`
  );
  // Style bar re-center since scale changes measurements.
  reCenterStyleBar();
}

document.addEventListener('DOMContentLoaded', () => {
  // Initial apply (must happen before first render for the editor line-height etc.)
  const a = loadAppearance();
  applyAppearance(a);
  // Close handlers
  document.getElementById('appearance-close')?.addEventListener('click', closeAppearance);
  document.getElementById('appearance-modal')?.querySelector('.settings-backdrop')?.addEventListener('click', closeAppearance);

  // Font chips
  const fontChips = document.querySelectorAll('#ap-font-family .ap-chip');
  fontChips.forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.font === a.font);
    btn.addEventListener('click', () => {
      fontChips.forEach((x) => x.classList.remove('active'));
      btn.classList.add('active');
      a.font = btn.dataset.font;
      applyAppearance(a);
      saveAppearance(a);
    });
  });
  // UI scale chips
  const scaleChips = document.querySelectorAll('#ap-ui-scale .ap-chip');
  scaleChips.forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.scale === a.scale);
    btn.addEventListener('click', () => {
      scaleChips.forEach((x) => x.classList.remove('active'));
      btn.classList.add('active');
      a.scale = btn.dataset.scale;
      applyAppearance(a);
      saveAppearance(a);
    });
  });
  // Image size chips
  const imgChips = document.querySelectorAll('#ap-image-size .ap-chip');
  imgChips.forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.imgsz === a.imageSize);
    btn.addEventListener('click', () => {
      imgChips.forEach((x) => x.classList.remove('active'));
      btn.classList.add('active');
      a.imageSize = btn.dataset.imgsz;
      applyAppearance(a);
      saveAppearance(a);
    });
  });
  // Line height slider
  const lh = document.getElementById('ap-line-height');
  const lhVal = document.getElementById('ap-line-height-val');
  if (lh) {
    lh.value = a.lineHeight;
    if (lhVal) lhVal.textContent = Number(a.lineHeight).toFixed(2);
    lh.addEventListener('input', () => {
      a.lineHeight = lh.value;
      if (lhVal) lhVal.textContent = Number(lh.value).toFixed(2);
      applyAppearance(a);
      saveAppearance(a);
    });
  }
  // Per-region font-size sliders
  const bindFs = (id, valId, key) => {
    const el = document.getElementById(id);
    const v = document.getElementById(valId);
    if (!el) return;
    el.value = String(a[key]);
    if (v) v.textContent = `${a[key]}px`;
    el.addEventListener('input', () => {
      a[key] = parseInt(el.value, 10);
      if (v) v.textContent = `${a[key]}px`;
      applyAppearance(a);
      saveAppearance(a);
    });
  };
  bindFs('ap-fs-base', 'ap-fs-base-val', 'fsBase');
  bindFs('ap-fs-right', 'ap-fs-right-val', 'fsRight');
  bindFs('ap-fs-sidebar', 'ap-fs-sidebar-val', 'fsSidebar');
  bindFs('ap-fs-toolbar', 'ap-fs-toolbar-val', 'fsToolbar');
  bindFs('ap-fs-status', 'ap-fs-status-val', 'fsStatus');
});

/* ─────────── Editor-border setting ─────────── */
const EDITOR_BORDER_KEY = 'leaf-ime:editor-border';
function applyEditorBorder(mode) {
  const m = ['none', 'hairline', 'paper'].includes(mode) ? mode : 'none';
  if (m === 'none') document.documentElement.removeAttribute('data-editor-border');
  else document.documentElement.setAttribute('data-editor-border', m);
  try { localStorage.setItem(EDITOR_BORDER_KEY, m); } catch (_) {}
}
try {
  const saved = localStorage.getItem(EDITOR_BORDER_KEY);
  if (saved) applyEditorBorder(saved);
} catch (_) {}
document.addEventListener('DOMContentLoaded', () => {
  const sel = document.getElementById('editor-border');
  if (!sel) return;
  const saved = (() => { try { return localStorage.getItem(EDITOR_BORDER_KEY); } catch { return null; } })();
  if (saved) sel.value = saved;
  sel.addEventListener('change', () => applyEditorBorder(sel.value));
});

/* ─────────── Status bar visibility ─────────── */
// Default is visible; the user can toggle it off via 보기 → 상태 표시줄
// 표시/숨김. The preference persists in localStorage. resetAllSettings
// clears the key so the default (visible) comes back.
const STATUSBAR_KEY = 'leaf-ime:statusbar-hidden';
let statusBarHidden = (() => {
  try { return localStorage.getItem(STATUSBAR_KEY) === '1'; } catch { return false; }
})();

function applyStatusBarVisibility() {
  const el = document.getElementById('status-bar');
  if (!el) return;
  el.classList.toggle('hidden', statusBarHidden);
}

async function toggleStatusBar() {
  let nextVisible;
  try {
    const v = await invoke('get_menu_check', { id: 'toggle-statusbar' });
    nextVisible = (typeof v === 'boolean') ? v : statusBarHidden;
  } catch { nextVisible = statusBarHidden; }
  statusBarHidden = !nextVisible;
  try { localStorage.setItem(STATUSBAR_KEY, statusBarHidden ? '1' : '0'); } catch (_) {}
  applyStatusBarVisibility();
  requestAnimationFrame(() => ensureCaretVisible?.());
}

document.addEventListener('DOMContentLoaded', () => {
  applyStatusBarVisibility();
  if (typeof syncMenuCheck === 'function') {
    syncMenuCheck('toggle-statusbar', !statusBarHidden);
  }
});

// 드래그 영역(.app-titlebar, .files-head, .tab-bar) 더블클릭 → 창 최대화
// ↔ 복원 토글. macOS 는 .app-titlebar 가 최상단 행이라 거기서 주로 동작
// 하지만, Windows/Linux 는 .app-titlebar 가 숨겨지므로 .files-head 와
// .tab-bar 도 동일 핸들러를 단다.
document.addEventListener('DOMContentLoaded', () => {
  const targets = [
    document.getElementById('app-titlebar'),
    document.querySelector('.files-pane > .files-head'),
    document.getElementById('tab-bar'),
  ].filter(Boolean);
  for (const el of targets) {
    el.addEventListener('dblclick', (ev) => {
      if (ev.target.closest('button, a, input, [contenteditable="true"], .tab')) return;
      invoke('toggle_window_maximize').catch(() => null);
    });
  }
});

/* ─────────── Style bar — persistent floating format toolbar ─────────── */
const STYLEBAR_KEY = 'leaf-ime:stylebar';
let styleBarVisible = (() => {
  try { return localStorage.getItem(STYLEBAR_KEY) === '1'; } catch { return false; }
})();

function applyStyleBarVisibility() {
  const bar = document.getElementById('style-bar');
  if (bar) bar.hidden = !styleBarVisible;
  reCenterStyleBar();
  if (typeof syncMenuCheck === 'function') {
    syncMenuCheck('toggle-stylebar', styleBarVisible);
  }
}

/* Center the floating style bar over the editor-wrap (not the whole
   viewport) — the file-tree sidebar on the left would otherwise make
   the viewport center look skewed relative to the actual writing
   column. Recomputes on every layout change that can affect the
   editor's horizontal position/size. */
function reCenterStyleBar() {
  const bar = document.getElementById('style-bar');
  if (!bar || bar.hidden) return;
  const wrap = document.querySelector('.editor-wrap');
  if (!wrap) return;
  const r = wrap.getBoundingClientRect();
  if (r.width === 0) return; // layout not ready
  const cx = r.left + r.width / 2;
  bar.style.left = `${cx}px`;
  bar.style.right = 'auto';
  bar.style.transform = 'translateX(-50%)';
}

// Observe layout changes on the editor-wrap so the bar follows when
// the left sidebar collapses, themes swap, window resizes, etc.
(function observeEditorWrap() {
  const wrap = document.querySelector('.editor-wrap');
  if (!wrap) return;
  if (typeof ResizeObserver !== 'undefined') {
    const ro = new ResizeObserver(() => reCenterStyleBar());
    ro.observe(wrap);
  }
  window.addEventListener('resize', reCenterStyleBar);
})();
async function toggleStyleBar() {
  // macOS `CheckMenuItem` 은 클릭 시 native 측 ✓ 가 먼저 auto-toggle 된다.
  // 우리가 JS 에서 `!state` 로 뒤집은 뒤 set_menu_check 로 덮어쓰면 그 값
  // 위에 macOS auto-toggle 이 한 번 더 적용돼 결과가 ✓ ↔ 실제 표시 가
  // 반대로 보이는 경우가 있다. 그래서 native 의 새 값을 진실로 읽고
  // syncMenuCheck 는 호출하지 않는다.
  let next;
  try {
    const v = await invoke('get_menu_check', { id: 'toggle-stylebar' });
    next = (typeof v === 'boolean') ? v : !styleBarVisible;
  } catch { next = !styleBarVisible; }
  styleBarVisible = next;
  try { localStorage.setItem(STYLEBAR_KEY, styleBarVisible ? '1' : '0'); } catch (_) {}
  // DOM 만 갱신. native ✓ 는 이미 auto-toggle 로 next 와 일치.
  const bar = document.getElementById('style-bar');
  if (bar) bar.hidden = !styleBarVisible;
  reCenterStyleBar();
  requestAnimationFrame(() => ensureCaretVisible());
}
// No-op stub — the style bar is now a format toolbar, not a stats display;
// renderCore still calls this every frame, so keep the symbol defined.
function updateStyleBar() { /* stats removed; bar is always-on formatting */ }

document.addEventListener('DOMContentLoaded', () => {
  applyStyleBarVisibility();

  // Share click dispatch with md-toolbar — same data-md actions.
  const sb = document.getElementById('style-bar');
  if (!sb) return;
  sb.addEventListener('mousedown', (ev) => ev.preventDefault()); // preserve caret/selection
  sb.addEventListener('click', async (ev) => {
    const btn = ev.target.closest('button[data-md]');
    if (!btn) return;
    ev.preventDefault();
    const act = btn.dataset.md;
    switch (act) {
      case 'bold': await mdWrap('**', '**'); break;
      case 'italic': await mdWrap('*', '*'); break;
      case 'strike': await mdWrap('~~', '~~'); break;
      case 'code': await mdWrap('`', '`'); break;
      case 'h1': await mdReplaceLinePrefix(mdToggleHeading(1)); break;
      case 'h2': await mdReplaceLinePrefix(mdToggleHeading(2)); break;
      case 'h3': await mdReplaceLinePrefix(mdToggleHeading(3)); break;
      case 'ul': await mdReplaceLinePrefix(mdToggleBullet()); break;
      case 'ol': await mdReplaceLinePrefix(mdToggleOrdered()); break;
      case 'task': await mdReplaceLinePrefix(mdToggleTask()); break;
      case 'quote': await mdReplaceLinePrefix(mdToggleQuote()); break;
      case 'link': await mdInsertLink(); break;
      case 'hr': await mdInsertHr(); break;
    }
    editor.focus();
  });
});

/* ─────────── Caret auto-scroll ─────────── */
function ensureCaretVisible() {
  const rect = caretRect();
  if (!rect) return;
  const wrap = editor.closest('.editor-wrap');
  if (!wrap) return;
  const wrapRect = wrap.getBoundingClientRect();

  // When the floating style bar is visible it covers the bottom strip
  // of the editor — use its top edge as the effective bottom so the
  // caret is scrolled above it rather than under it. Keep a small
  // buffer so the caret doesn't kiss the toolbar.
  let effectiveBottom = wrapRect.bottom;
  const sb = document.getElementById('style-bar');
  if (sb && !sb.hidden) {
    const sbRect = sb.getBoundingClientRect();
    if (sbRect.top > wrapRect.top && sbRect.top < effectiveBottom) {
      effectiveBottom = sbRect.top - 8;
    }
  }

  const margin = Math.min(80, Math.max(32, rect.height + 16));
  if (rect.bottom > effectiveBottom - margin) {
    wrap.scrollTop += (rect.bottom - effectiveBottom) + margin;
  } else if (rect.top < wrapRect.top + margin) {
    wrap.scrollTop -= (wrapRect.top - rect.top) + margin;
  }
}
(async () => {
  for (let i = 0; i < 10; i++) {
    const listen = window.__TAURI__?.event?.listen;
    if (listen) {
      try {
        await listen('menu-action', (ev) => handleMenuAction(String(ev.payload || '')));
      } catch (e) {
        console.warn('menu-action listen failed:', e);
      }
      try {
        await listen('quit-requested', () => {
          handleQuitRequested().catch((err) => console.error('quit failed:', err));
        });
      } catch (e) {
        console.warn('quit-requested listen failed:', e);
      }
      return;
    }
    await new Promise((r) => setTimeout(r, 120));
  }
  console.warn('Tauri event API unavailable');
})();

// 창 X 버튼/⌘Q 등으로 종료가 요청되면 Rust 쪽에서 close/exit 를 막은 뒤
// 이 이벤트를 보낸다. 저장 안 된 탭이 있으면 사용자에게 묻고, 결정에
// 따라 `quit_app` 으로 다시 돌아가 실제 종료를 트리거한다. `quit_app`
// 은 곧바로 프로세스를 내리므로 응답이 돌아오지 않을 수 있어 await 하지
// 않고 fire-and-forget 한다.
let quitInProgress = false;
function fireQuit() {
  invoke('quit_app').catch((err) => console.error('quit_app failed:', err));
}
async function handleQuitRequested() {
  if (quitInProgress) return;
  quitInProgress = true;
  try {
    commitTabState();
    const dirty = tabs.filter((t) => t.dirty);
    if (dirty.length === 0) {
      fireQuit();
      return;
    }
    // 탭별로 차례차례 묻는다. 어느 탭에 대해 묻는지 즉시 보이도록
    // 해당 탭을 먼저 활성화한 뒤 모달을 띄운다.
    for (let i = 0; i < dirty.length; i++) {
      const t = dirty[i];
      if (t.id !== activeTabId) {
        commitTabState();
        loadTabState(t.id);
      }
      const badge = dirty.length > 1 ? ` (${i + 1}/${dirty.length})` : '';
      const answer = await askConfirm({
        title: `"${t.title}" 에 저장되지 않은 변경 사항이 있습니다${badge}`,
        body: t.path
          ? '저장하고 종료할까요?'
          : '새 파일로 저장하거나 변경 사항을 버리고 종료할 수 있습니다.',
      });
      if (answer === 'cancel') return;
      if (answer === 'save') {
        const ok = await saveTab(t);
        if (!ok) return;
      }
      // 'discard': 이 탭은 저장하지 않고 다음 탭으로 넘어간다.
    }
    fireQuit();
  } finally {
    quitInProgress = false;
  }
}

// ── 자동 업데이트 ────────────────────────────────────────────────
// 시작 후 한 번, 그리고 사용자가 메뉴에서 트리거할 때마다 GitHub
// Releases 의 latest.json 매니페스트를 조회한다. pubkey 가 자리표시자
// 상태이거나 네트워크 오류가 나면 조용히 넘어간다 — 에디터 사용을
// 막아서는 안 된다.
let updatePromptInProgress = false;
async function promptForUpdateIfAny({ silentIfNone = true } = {}) {
  if (updatePromptInProgress) return;
  updatePromptInProgress = true;
  try {
    let info;
    try {
      info = await invoke('check_for_update');
    } catch (e) {
      if (!silentIfNone) {
        await askConfirm({
          title: '업데이트 확인 실패',
          body: `원격 매니페스트를 조회하지 못했습니다.\n\n${e}`,
          saveLabel: '확인',
          discardLabel: '',
          cancelLabel: '',
        });
      } else {
        console.warn('업데이트 확인 실패(무시):', e);
      }
      return;
    }
    if (!info) {
      if (!silentIfNone) {
        await askConfirm({
          title: '최신 버전입니다',
          body: '현재 설치된 버전이 가장 최신입니다.',
          saveLabel: '확인',
          discardLabel: '',
          cancelLabel: '',
        });
      }
      return;
    }
    const answer = await askConfirm({
      title: `새 버전 ${info.version} 이 있습니다`,
      body:
        `현재 ${info.current_version} → ${info.version}` +
        (info.date ? ` (${info.date})` : '') +
        '\n\n' +
        (info.body ? `${info.body}\n\n` : '') +
        '지금 다운로드·설치하고 다시 시작할까요?',
      saveLabel: '지금 설치',
      discardLabel: '나중에',
      cancelLabel: '',
    });
    if (answer === 'save') {
      logEvent(`업데이트 다운로드 중 (${info.version})…`);
      try {
        await invoke('install_update');
        // 정상 경로면 install_update 안에서 app.restart() 가 호출돼
        // 여기까지 오지 않는다.
      } catch (e) {
        await askConfirm({
          title: '업데이트 설치 실패',
          body: `${e}\n\n잠시 후 다시 시도하거나, 공식 페이지에서 수동으로 받아 주세요.`,
          saveLabel: '확인',
          discardLabel: '',
          cancelLabel: '',
        });
      }
    }
  } finally {
    updatePromptInProgress = false;
  }
}
// 부팅 직후엔 사용자에게 방해가 안 되도록 살짝 늦춘다.
setTimeout(() => { promptForUpdateIfAny({ silentIfNone: true }); }, 4000);

// ── Abbreviation suggestion state ────────────────────────────────
let suggestionItems = [];
let suggestionIndex = 0;
let suggestionsManuallyDismissed = false;

// Current Hangul layout id. When it's `'os-ime'`, sbmd stops intercepting
// keystrokes and lets the system IME handle them natively.
let currentHangulLayoutId = '';
function isOsImeMode() {
  return currentHangulLayoutId === 'os-ime';
}

function isHelpOpen() {
  return !helpModal.classList.contains('hidden');
}
function openHelp() {
  helpModal.classList.remove('hidden');
  helpClose.focus();
}
function closeHelp() {
  helpModal.classList.add('hidden');
  editor.focus();
}

// `helpBtn` is null because the header icon button was removed in favor
// of the native application menu — but `helpClose` / `helpBackdrop`
// still exist inside the help modal and need their listeners.
helpBtn?.addEventListener('click', openHelp);
helpClose?.addEventListener('click', closeHelp);
helpBackdrop?.addEventListener('click', closeHelp);

/* ─────────── About modal (custom, animated) ─────────── */
const aboutModal = document.getElementById('about-modal');
const aboutClose = document.getElementById('about-close');
const aboutBackdrop = aboutModal?.querySelector('.about-backdrop');
const aboutVersionEl = document.getElementById('about-version');

function isAboutOpen() {
  return aboutModal && !aboutModal.classList.contains('hidden');
}
function openAbout() {
  if (!aboutModal) return;
  // Re-trigger CSS animations by removing then adding the hidden class on
  // the NEXT tick. Without this, a user opening About a second time sees
  // a static panel — animations only fire when the element becomes visible.
  aboutModal.classList.add('hidden');
  requestAnimationFrame(() => {
    aboutModal.classList.remove('hidden');
    aboutClose?.focus();
  });
}
function closeAbout() {
  if (!aboutModal) return;
  aboutModal.classList.add('hidden');
  editor.focus();
}

aboutClose?.addEventListener('click', closeAbout);
aboutBackdrop?.addEventListener('click', closeAbout);
// "made with ♥ · leaf" — leaf 를 누르면 프로젝트 저장소를 기본 브라우저로.
document.getElementById('about-leaf-link')?.addEventListener('click', (ev) => {
  ev.preventDefault();
  const url = ev.currentTarget.dataset.url;
  if (url) invoke('open_url', { url }).catch(() => null);
});
// Close on Esc when the About modal is the top-most modal.
window.addEventListener('keydown', (ev) => {
  if (ev.key === 'Escape' && isAboutOpen()) {
    ev.preventDefault();
    ev.stopPropagation();
    closeAbout();
  }
}, true);
// Keep the version label in sync with the Tauri app version so we don't
// hand-maintain two sources of truth.
(async () => {
  if (!aboutVersionEl) return;
  try {
    const v = await invoke('app_version').catch(() => null);
    if (v) aboutVersionEl.textContent = `v${v}`;
  } catch (_) {}
})();

/* ─────────── Settings modal ─────────── */
const settingsBtn = document.getElementById('settings-btn');
const settingsModal = document.getElementById('settings-modal');
const settingsClose = document.getElementById('settings-close');
const settingsBackdrop = settingsModal?.querySelector('.settings-backdrop');

function isSettingsOpen() {
  return settingsModal && !settingsModal.classList.contains('hidden');
}
function openSettings() {
  settingsModal.classList.remove('hidden');
  settingsClose?.focus();
}
function closeSettings() {
  settingsModal.classList.add('hidden');
  editor.focus();
}
settingsBtn?.addEventListener('click', openSettings);
settingsClose?.addEventListener('click', closeSettings);
settingsBackdrop?.addEventListener('click', closeSettings);

/* ─────────── Sidebar panel visibility ─────────── */
const sideInfoEl = document.getElementById('side-info');
const kbPanelEl = document.getElementById('kb-panel');
const abbrPanelEl = document.getElementById('abbr-panel');
const SIDEBAR_KEY = 'leaf-ime:sidebar';
// 우측 보조 사이드바 패널은 보기 메뉴에서 토글한다. 상태는 localStorage 의
// {kb, ab} 로 보관 (기존 키 그대로 사용해 사용자 설정 유지).
let keymapPanelVisible = false;
let abbrsPanelVisible = false;
try {
  const saved = localStorage.getItem(SIDEBAR_KEY);
  if (saved) {
    const { kb, ab } = JSON.parse(saved);
    keymapPanelVisible = !!kb;
    abbrsPanelVisible = !!ab;
  }
} catch (_) {}
function applySidebarPanelVisibility(opts = {}) {
  if (kbPanelEl) kbPanelEl.hidden = !keymapPanelVisible;
  if (abbrPanelEl) abbrPanelEl.hidden = !abbrsPanelVisible;
  if (sideInfoEl) {
    sideInfoEl.classList.toggle('empty', !keymapPanelVisible && !abbrsPanelVisible);
  }
  try {
    localStorage.setItem(SIDEBAR_KEY, JSON.stringify({
      kb: keymapPanelVisible,
      ab: abbrsPanelVisible,
    }));
  } catch (_) {}
  // 메뉴 체크 동기화 — 메뉴 클릭 핸들러에서는 native 가 이미 auto-toggle
  // 로 맞춰져 있어 또 set_menu_check 를 부르면 macOS 가 그 위에 한 번 더
  // toggle 해 결과가 반전된다. opts.skipMenuSync 로 그 경로에서 차단.
  if (!opts.skipMenuSync && typeof syncMenuCheck === 'function') {
    syncMenuCheck('toggle-keymap-panel', keymapPanelVisible);
    syncMenuCheck('toggle-abbrs-panel', abbrsPanelVisible);
  }
}
// macOS CheckMenuItem auto-toggle 와의 경쟁 회피 — 클릭 핸들러에서는
// native 의 새 ✓ 값을 진실로 읽어 그 방향으로 정한다.
async function toggleKeymapPanel() {
  let next;
  try {
    const v = await invoke('get_menu_check', { id: 'toggle-keymap-panel' });
    next = (typeof v === 'boolean') ? v : !keymapPanelVisible;
  } catch { next = !keymapPanelVisible; }
  keymapPanelVisible = next;
  applySidebarPanelVisibility({ skipMenuSync: true });
}
async function toggleAbbrsPanel() {
  let next;
  try {
    const v = await invoke('get_menu_check', { id: 'toggle-abbrs-panel' });
    next = (typeof v === 'boolean') ? v : !abbrsPanelVisible;
  } catch { next = !abbrsPanelVisible; }
  abbrsPanelVisible = next;
  applySidebarPanelVisibility({ skipMenuSync: true });
}
applySidebarPanelVisibility();

// Global key listener for F1 (open help) and Esc (close modals when open).
// Runs at window level so it takes precedence over the editor handler.
window.addEventListener('keydown', (ev) => {
  if (ev.key === 'F1') {
    ev.preventDefault();
    if (isHelpOpen()) closeHelp(); else openHelp();
    return;
  }
  if (ev.key === 'Escape') {
    if (isSettingsOpen()) {
      ev.preventDefault();
      ev.stopPropagation();
      closeSettings();
      return;
    }
    if (isHelpOpen()) {
      ev.preventDefault();
      ev.stopPropagation();
      closeHelp();
    }
  }
  // ⌘, / Ctrl+, — open settings (macOS / general convention)
  if ((ev.metaKey || ev.ctrlKey) && ev.key === ',') {
    ev.preventDefault();
    if (isSettingsOpen()) closeSettings(); else openSettings();
  }
}, true);

// Row layout for the virtual keyboard panel. Each entry is
// `[rowIndent, codes[]]`; indent adds a small left offset like a real
// staggered keyboard.
const KB_ROWS = [
  [0, ['Digit1','Digit2','Digit3','Digit4','Digit5','Digit6','Digit7','Digit8','Digit9','Digit0','Minus','Equal']],
  [1, ['KeyQ','KeyW','KeyE','KeyR','KeyT','KeyY','KeyU','KeyI','KeyO','KeyP','BracketLeft','BracketRight']],
  [2, ['KeyA','KeyS','KeyD','KeyF','KeyG','KeyH','KeyJ','KeyK','KeyL','Semicolon','Quote']],
  [3, ['KeyZ','KeyX','KeyC','KeyV','KeyB','KeyN','KeyM','Comma','Period','Slash']],
];

async function renderKeyboardMap() {
  if (!keyboardMapEl) return;
  if (isOsImeMode()) {
    keyboardMapEl.innerHTML = '';
    const note = document.createElement('div');
    note.className = 'os-ime-note';
    note.textContent = '시스템 IME 사용 중 — OS 설정에 따라 타이핑됩니다.';
    keyboardMapEl.appendChild(note);
    return;
  }
  const hints = await invoke('layout_map');
  const byCode = new Map(hints.map((h) => [h.code, h]));
  keyboardMapEl.innerHTML = '';
  for (const [indent, codes] of KB_ROWS) {
    const row = document.createElement('div');
    row.className = `kb-row${indent ? ` kb-row--indent-${indent}` : ''}`;
    for (const code of codes) {
      const h = byCode.get(code);
      const cell = document.createElement('div');
      cell.className = 'kb-key';
      if (h?.role) cell.dataset.role = h.role;
      const phys = document.createElement('span');
      phys.className = 'phys';
      phys.textContent = labelFor(code);
      const out = document.createElement('span');
      out.className = 'out';
      out.textContent = h?.base || '';
      cell.appendChild(phys);
      cell.appendChild(out);
      if (h?.shift && h.shift !== h.base) {
        const sh = document.createElement('span');
        sh.className = 'shift';
        sh.textContent = h.shift;
        cell.appendChild(sh);
      }
      row.appendChild(cell);
    }
    keyboardMapEl.appendChild(row);
  }
}

function labelFor(code) {
  if (code.startsWith('Key')) return code.slice(3);
  if (code.startsWith('Digit')) return code.slice(5);
  return ({
    Minus: '-', Equal: '=',
    BracketLeft: '[', BracketRight: ']',
    Semicolon: ';', Quote: "'",
    Comma: ',', Period: '.', Slash: '/',
    Backslash: '\\', Backquote: '`',
  })[code] || code;
}

let committed = '';
let preedit = '';
// Source-string character index where the caret currently sits. The IME
// preedit is inserted into the DOM at this position during render, so
// fresh committed text flows from the right spot rather than always
// appending to the end.
let cursor = 0;
// Markdown WYSIWYG on/off. When on, `committed` is rendered as styled
// HTML (headings, lists, quotes, bold/italic markers, etc.) and the
// shortcut layer + floating toolbar become active. Default is ON; the
// user's most-recent choice is persisted to localStorage.
let markdownMode = true;
const MD_KEY = 'leaf-ime:markdown';
try {
  const saved = localStorage.getItem(MD_KEY);
  if (saved !== null) markdownMode = saved === '1';
} catch (_) {}

function render() {
  // In OS IME mode the editor DOM is the source of truth (native IME
  // writes directly into it), so we must not rebuild it from our state.
  if (isOsImeMode()) {
    preeditDebugEl.textContent = '—';
    return;
  }
  renderCore();
}

function renderCore() {
  // Selected-image state points at a DOM element that the reset below
  // detaches; clear it so the pointer doesn't dangle.
  if (selectedImage) selectedImage = null;
  editor.innerHTML = '';
  editor.classList.toggle('md-mode', markdownMode);

  if (markdownMode) {
    renderMarkdownInto(editor, committed, preedit, cursor);
  } else {
    renderPlainInto(editor, committed, preedit, cursor);
  }

  // Restore caret at source position cursor + preedit.length (caret sits
  // after any preedit span we just inserted).
  placeCaretAtSourceIdx(cursor + (preedit ? preedit.length : 0));

  preeditDebugEl.textContent = preedit || '—';

  // 검색 바가 열려 있으면 render 로 DOM 이 교체되면서 하이라이트용 Range
  // 들이 무효해지므로 매번 다시 만든다. 검색 바가 닫혀 있으면 no-op.
  if (typeof rebuildFindHighlights === 'function') rebuildFindHighlights();
}

/* ─────────── Plain (non-markdown) rendering ─────────── */
function renderPlainInto(container, source, preeditText, cursorIdx) {
  const before = source.slice(0, cursorIdx);
  const after = source.slice(cursorIdx);
  appendTextWithBrs(container, before);
  if (preeditText) {
    const span = document.createElement('span');
    span.className = 'preedit';
    span.textContent = preeditText;
    container.appendChild(span);
  }
  appendTextWithBrs(container, after);
  // Make sure a trailing newline renders as a visible empty line.
  if (source.endsWith('\n') && cursorIdx === source.length && !preeditText) {
    container.appendChild(document.createElement('br'));
  }
}

function appendTextWithBrs(container, text) {
  if (!text) return;
  const lines = text.split('\n');
  for (let i = 0; i < lines.length; i++) {
    if (i > 0) container.appendChild(document.createElement('br'));
    if (lines[i]) container.appendChild(document.createTextNode(lines[i]));
  }
}

/* ─────────── Markdown rendering (block + inline) ─────────── */
function renderMarkdownInto(container, source, preeditText, cursorIdx) {
  const lines = source.split('\n');
  let fenced = false;
  let fenceLang = '';
  // Sequential index of the current open fence in source order.
  // fence-open 과 fence-close 양쪽에 같은 값을 걸어 놓으면 드롭다운에서
  // "N번째 블록의 언어를 바꿔라" → 소스 라인을 찾아 재작성하는 길이
  // 단순해진다.
  let fenceIdx = -1;
  let table = null; // { aligns: string[] | null, rowIdx: 0 }
  // 수식 블록 ($$ ... $$) 상태. mermaid/code 와 같은 async 프리뷰 패턴.
  let mathed = false;
  let mathIdx = -1;
  // GFM 알림 (> [!KIND]) — 첫 라인의 [!KIND] 를 읽어 뒤따르는 blockquote
  // 라인에 같은 종류 클래스를 옮겨 CSS 로 스타일링.
  let admonKind = null; // 'note' | 'tip' | 'important' | 'warning' | 'caution'
  // 문서 맨 앞의 YAML front matter ( --- / ... / --- ). 최초 비-빈 라인이
  // `---` 일 때만 열고, 닫는 `---` 를 만날 때까지 .md-yaml 로 표시.
  let yamlState = null; // null | 'body'
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fenceMatch = !fenced && /^\s*```/.test(line);
    const fenceClose = fenced && /^\s*```/.test(line);
    let block;
    if (fenceMatch) {
      fenced = true;
      table = null;
      fenceIdx += 1;
      fenceLang = line.replace(/^\s*```/, '').trim();
      block = makeLineBlock('md-line md-fence md-fence-open');
      if (fenceLang) block.dataset.lang = fenceLang;
      block.dataset.fenceIdx = String(fenceIdx);
      const s = document.createElement('span');
      s.className = 'md-marker md-syn';
      s.textContent = line;
      block.appendChild(s);
    } else if (fenceClose) {
      fenced = false;
      block = makeLineBlock('md-line md-fence md-fence-close');
      block.dataset.fenceIdx = String(fenceIdx);
      if (fenceLang) block.dataset.lang = fenceLang;
      const s = document.createElement('span');
      s.className = 'md-marker md-syn';
      s.textContent = line;
      block.appendChild(s);
      // 우하단 언어 선택 드롭다운. contenteditable=false 로 에디터의
      // 캐럿 영역 바깥임을 알려서 내부 편집 이벤트를 타지 않게 한다.
      block.appendChild(buildCodeLangSelect(fenceIdx, fenceLang));
      fenceLang = '';
    } else if (fenced) {
      block = makeLineBlock('md-line md-code-line');
      if (fenceLang) block.dataset.lang = fenceLang;
      if (line) {
        block.appendChild(document.createTextNode(line));
      } else {
        // 빈 코드 라인에 ' ' placeholder 를 넣으면 textContent 가 1 이 돼서
        // 실제 소스 길이(0) 와 어긋난다. 그 차이는 이후 모든 라인의 커서
        // 인덱스에 드리프트로 쌓여 "타이핑 중 커서가 다른 곳으로 튀는"
        // 증상을 만든다. <br> 는 textContent 0 이면서도 라인 높이를 확보.
        block.appendChild(document.createElement('br'));
      }
    } else if (!mathed && /^\$\$(.+?)\$\$\s*$/.test(line)) {
      // 단일 라인 수식 블록 ($$latex$$). 여는/본문/닫는 역할을 한 DOM 에
      // 담고, data-latex 로 수식 원문을 보관해 KaTeX 프리뷰가 뒤에 붙는다.
      const m = line.match(/^\$\$(.+?)\$\$\s*$/);
      table = null;
      mathIdx += 1;
      block = makeLineBlock('md-line md-math md-math-single');
      block.dataset.mathIdx = String(mathIdx);
      block.dataset.latex = m[1];
      const openMark = document.createElement('span');
      openMark.className = 'md-marker md-syn';
      openMark.textContent = '$$';
      block.appendChild(openMark);
      const body = document.createElement('span');
      body.className = 'md-math-body-inline';
      body.textContent = m[1];
      block.appendChild(body);
      const closeMark = document.createElement('span');
      closeMark.className = 'md-marker md-syn';
      closeMark.textContent = '$$';
      block.appendChild(closeMark);
    } else if (!mathed && /^\$\$\s*$/.test(line)) {
      // 수식 블록 시작 ($$)
      mathed = true;
      table = null;
      mathIdx += 1;
      block = makeLineBlock('md-line md-math md-math-open');
      block.dataset.mathIdx = String(mathIdx);
      const s = document.createElement('span');
      s.className = 'md-marker md-syn';
      s.textContent = line;
      block.appendChild(s);
    } else if (mathed && /^\$\$\s*$/.test(line)) {
      // 수식 블록 종료 ($$)
      mathed = false;
      block = makeLineBlock('md-line md-math md-math-close');
      block.dataset.mathIdx = String(mathIdx);
      const s = document.createElement('span');
      s.className = 'md-marker md-syn';
      s.textContent = line;
      block.appendChild(s);
    } else if (mathed) {
      // 수식 본문 — 텍스트 그대로 소스 역할을 겸하고, 뒤이어 async 로
      // KaTeX 프리뷰가 닫는 $$ 다음에 덧붙는다.
      block = makeLineBlock('md-line md-math md-math-body');
      if (line) block.appendChild(document.createTextNode(line));
      else block.appendChild(document.createElement('br'));
    } else if (yamlState === null && i === 0 && /^---\s*$/.test(line)) {
      // YAML front matter 열기. 오직 파일 첫 줄이 `---` 일 때만 시작.
      yamlState = 'body';
      block = makeLineBlock('md-line md-yaml md-yaml-open');
      const s = document.createElement('span');
      s.className = 'md-marker md-syn';
      s.textContent = line;
      block.appendChild(s);
    } else if (yamlState === 'body' && /^---\s*$/.test(line)) {
      // YAML front matter 닫기.
      yamlState = null;
      block = makeLineBlock('md-line md-yaml md-yaml-close');
      const s = document.createElement('span');
      s.className = 'md-marker md-syn';
      s.textContent = line;
      block.appendChild(s);
    } else if (yamlState === 'body') {
      // YAML body — 스타일만 입히고 내용은 그대로 노출.
      block = makeLineBlock('md-line md-yaml md-yaml-body');
      if (line) block.appendChild(document.createTextNode(line));
      else block.appendChild(document.createElement('br'));
    } else {
      // Table block detection — header + separator row pair initiates
      // a streak of rows until a non-table line ends it.
      if (!table && isTableRow(line) && i + 1 < lines.length && isTableSeparatorRow(lines[i + 1])) {
        table = { aligns: parseTableAligns(lines[i + 1]), rowIdx: 0, wrap: null, tbody: null };
      } else if (table && !isTableRow(line)) {
        table = null;
      }
      if (table && isTableRow(line)) {
        const isSep = isTableSeparatorRow(line);
        // 진짜 <table><tbody> 래퍼가 없으면 지금 만들어서 컨테이너에 붙인다.
        if (!table.wrap) {
          const tbl = document.createElement('table');
          tbl.className = 'md-table';
          const tb = document.createElement('tbody');
          tbl.appendChild(tb);
          container.appendChild(tbl);
          table.wrap = tbl;
          table.tbody = tb;
        }
        const row = renderTableRow(line, table.aligns, table.rowIdx, isSep);
        table.tbody.appendChild(row);
        table.rowIdx += 1;
        // 아래의 `container.appendChild(block)` 을 건너뛴다.
        block = null;
      } else {
        block = renderMdLineBlock(line);
        // GFM 알림 (admonition) 처리: blockquote 라인의 첫 내용이
        // [!NOTE]/[!TIP]/[!IMPORTANT]/[!WARNING]/[!CAUTION] 이면
        // 그 라인과 이어지는 blockquote 들에 같은 종류 클래스를 입힘.
        if (block && block.classList.contains('md-quote')) {
          const content = line.replace(/^\s*> ?/, '');
          const adm = content.match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*$/i);
          if (adm) {
            admonKind = adm[1].toLowerCase();
            block.classList.add('md-admonition', 'md-admonition-head',
                                `md-admonition-${admonKind}`);
          } else if (admonKind) {
            block.classList.add('md-admonition', `md-admonition-${admonKind}`);
          }
        } else {
          admonKind = null;
        }
      }
    }
    if (block) container.appendChild(block);
  }

  // Mark always-hidden syntax pieces as non-editable atoms so browser
  // arrow-key navigation skips over them. Without this, the caret can
  // land inside zero-width `.md-syn` spans, causing Left/Right to
  // "do nothing" visually and Up/Down to pick the wrong column/line
  // (the caret's x-coordinate is taken from the invisible span, not
  // from where the user sees it). `.md-table-sep` rows are height: 0
  // and would otherwise count as an extra line during vertical nav.
  container.querySelectorAll('.md-syn, .md-table-sep').forEach((el) => {
    el.contentEditable = 'false';
  });

  // Preedit: walk the freshly built DOM and insert a preedit span at
  // the cursor's source index (counting +1 per line boundary for the
  // implicit '\n').
  if (preeditText) insertPreeditAtSourceIdx(container, cursorIdx, preeditText);
}

/* ─────────── Table helpers ─────────── */
function isTableRow(line) {
  const s = line.trim();
  if (s.length < 3 || !s.includes('|')) return false;
  // Require a pipe somewhere in the middle (not just leading/trailing).
  const stripped = s.replace(/^\|/, '').replace(/\|$/, '');
  return stripped.includes('|');
}
function isTableSeparatorRow(line) {
  const s = line.trim();
  if (!s.includes('|')) return false;
  const cells = s.replace(/^\|/, '').replace(/\|$/, '').split('|');
  if (!cells.length) return false;
  // GFM allows any number of dashes (one or more) with optional colons —
  // `:-:`, `:---:`, `---`, `-` are all valid alignment rows.
  return cells.every((c) => /^\s*:?-+:?\s*$/.test(c));
}
function parseTableAligns(sep) {
  const s = sep.trim();
  const cells = s.replace(/^\|/, '').replace(/\|$/, '').split('|');
  return cells.map((c) => {
    const t = c.trim();
    const l = t.startsWith(':');
    const r = t.endsWith(':');
    if (l && r) return 'center';
    if (r) return 'right';
    return 'left';
  });
}
/* ─────────── Code block language selector ─────────── */
// 드롭다운에 노출할 언어 목록. value 는 마크다운 소스에 들어갈
// 식별자(```<value>) 이고 label 은 사람이 읽는 이름. 'plain' 은
// 언어 없음(```). 에일리어스(js/ts 등)는 동일 언어로 매핑되므로
// 여기선 정식 이름 하나만 제공한다.
const CODE_LANG_OPTIONS = [
  { value: '', label: '일반 텍스트' },
  { value: 'markdown', label: 'Markdown' },
  { value: 'html', label: 'HTML' },
  { value: 'css', label: 'CSS' },
  { value: 'javascript', label: 'JavaScript' },
  { value: 'typescript', label: 'TypeScript' },
  { value: 'python', label: 'Python' },
  { value: 'rust', label: 'Rust' },
  { value: 'go', label: 'Go' },
  { value: 'java', label: 'Java' },
  { value: 'kotlin', label: 'Kotlin' },
  { value: 'swift', label: 'Swift' },
  { value: 'c', label: 'C' },
  { value: 'cpp', label: 'C++' },
  { value: 'csharp', label: 'C#' },
  { value: 'ruby', label: 'Ruby' },
  { value: 'php', label: 'PHP' },
  { value: 'sql', label: 'SQL' },
  { value: 'bash', label: 'Bash' },
  { value: 'json', label: 'JSON' },
  { value: 'yaml', label: 'YAML' },
  { value: 'toml', label: 'TOML' },
  { value: 'xml', label: 'XML' },
  { value: 'mermaid', label: 'Mermaid' },
];

// 언어 옵션과 별개로 "코드 블록에 가하는 동작" 을 같은 드롭다운에서
// 선택하게 하려고 센티넬 value 를 분리해 둔다. applyCodeLangChange 는
// 이 값이 들어오면 안 되니 change 핸들러에서 먼저 가로채 처리한다.
const CODE_ACTION_TRIM = '__action_trim__';

function buildCodeLangSelect(fenceIdx, currentLang) {
  const sel = document.createElement('select');
  sel.className = 'md-code-lang-select';
  sel.setAttribute('contenteditable', 'false');
  sel.setAttribute('tabindex', '-1');
  sel.setAttribute('aria-label', '코드 블록 언어 선택');
  sel.dataset.fenceIdx = String(fenceIdx);

  const cur = (currentLang || '').trim().toLowerCase();
  const hasPreset = CODE_LANG_OPTIONS.some((o) => o.value === cur);

  // 동작 그룹: 코드 블록에 바로 적용하는 one-shot 액션들.
  // 선택 후 바로 value 를 현재 언어로 되돌려서 "옵션이 선택된 상태로
  // 남는" 어색함을 없앤다.
  const actionGroup = document.createElement('optgroup');
  actionGroup.label = '동작';
  const trimOpt = document.createElement('option');
  trimOpt.value = CODE_ACTION_TRIM;
  trimOpt.textContent = '앞뒤 공백 제거';
  actionGroup.appendChild(trimOpt);
  sel.appendChild(actionGroup);

  const langGroup = document.createElement('optgroup');
  langGroup.label = '언어';
  // 목록에 없는 언어(dart, elixir 등)도 그대로 유지·표시되도록 옵션으로 주입.
  if (cur && !hasPreset) {
    const opt = document.createElement('option');
    opt.value = cur;
    opt.textContent = cur;
    langGroup.appendChild(opt);
  }
  for (const o of CODE_LANG_OPTIONS) {
    const opt = document.createElement('option');
    opt.value = o.value;
    opt.textContent = o.label;
    langGroup.appendChild(opt);
  }
  sel.appendChild(langGroup);
  sel.value = cur;

  // 에디터가 마우스다운을 자기 것으로 가로채 캐럿을 옮기지 않도록 차단.
  sel.addEventListener('mousedown', (ev) => ev.stopPropagation());
  sel.addEventListener('click', (ev) => ev.stopPropagation());
  sel.addEventListener('change', () => {
    if (sel.value === CODE_ACTION_TRIM) {
      // 액션은 "선택 상태" 로 남겨 두지 않는다. 먼저 value 를 되돌려
      // render() 이후에도 현재 언어가 표시되도록.
      sel.value = cur;
      trimCodeBlock(fenceIdx);
      return;
    }
    applyCodeLangChange(fenceIdx, sel.value);
  });
  return sel;
}

// N번째 코드 블록의 각 라인에 trim() 을 적용한다. 소스에 남아 있던
// 앞뒤 공백/탭을 그대로 한 번에 정리하는 용도. 커서는 가능한 한 자연스러운
// 지점으로 옮기고 (블록 밖은 delta 만큼 쉬프트, 블록 내부는 첫 코드 라인
// 머리로) 캐럿이 "허공" 에 떨어지지 않도록 한다.
function trimCodeBlock(fenceIdx) {
  const lines = committed.split('\n');
  let seen = -1;
  let startLineIdx = -1;
  let endLineIdx = -1;
  let inFence = false;
  for (let i = 0; i < lines.length; i++) {
    const isFence = /^\s*```/.test(lines[i]);
    if (isFence && !inFence) {
      inFence = true;
      seen += 1;
      if (seen === fenceIdx) startLineIdx = i;
    } else if (isFence && inFence) {
      inFence = false;
      if (seen === fenceIdx) { endLineIdx = i; break; }
    }
  }
  // 코드 블록이 닫히지 않았거나(endLineIdx < 0) 내용이 아예 없는 경우
  // (endLineIdx === startLineIdx + 1) 는 할 일이 없다.
  if (startLineIdx < 0 || endLineIdx <= startLineIdx + 1) return;

  // 커서 보정용 원본 라인 길이.
  const origLens = lines.map((l) => l.length);
  let firstCodeStart = 0;
  for (let k = 0; k <= startLineIdx; k++) firstCodeStart += origLens[k] + 1;
  let closeFenceStart = firstCodeStart;
  for (let k = startLineIdx + 1; k < endLineIdx; k++) closeFenceStart += origLens[k] + 1;

  let changed = false;
  for (let k = startLineIdx + 1; k < endLineIdx; k++) {
    const trimmed = lines[k].trim();
    if (trimmed !== lines[k]) { lines[k] = trimmed; changed = true; }
  }
  if (!changed) return;

  snapshot();
  const newSource = lines.join('\n');
  const delta = newSource.length - committed.length;
  committed = newSource;
  if (cursor >= closeFenceStart) {
    cursor += delta;
  } else if (cursor >= firstCodeStart) {
    // 트림으로 각 라인의 시작/끝이 바뀌었으니 안전하게 블록 머리로 둔다.
    cursor = firstCodeStart;
  }
  cursor = Math.max(0, Math.min(cursor, committed.length));
  commitTabState();
  render();
  editor.focus();
}

// fence-open 의 N번째 라인을 찾아 ```뒤의 언어 표시를 교체한다.
// cursor 가 그 라인 뒤에 있다면 길이 차이만큼 밀어준다.
function applyCodeLangChange(fenceIdx, newLang) {
  const lines = committed.split('\n');
  let seen = -1;
  let inFence = false;
  for (let i = 0; i < lines.length; i++) {
    const isFence = /^\s*```/.test(lines[i]);
    if (isFence && !inFence) {
      seen += 1;
      if (seen === fenceIdx) {
        const leading = (lines[i].match(/^\s*/) || [''])[0];
        const oldLine = lines[i];
        const newLine = `${leading}\`\`\`${newLang || ''}`;
        if (oldLine === newLine) return;
        // 이 라인의 소스 시작 오프셋.
        let lineStart = 0;
        for (let k = 0; k < i; k++) lineStart += lines[k].length + 1;
        const lineEnd = lineStart + oldLine.length;
        const delta = newLine.length - oldLine.length;
        snapshot();
        lines[i] = newLine;
        committed = lines.join('\n');
        if (cursor > lineEnd) cursor += delta;
        else if (cursor > lineStart) cursor = lineStart + newLine.length;
        commitTabState();
        render();
        // 드롭다운으로 인해 에디터에서 포커스가 빠졌을 수 있으므로
        // 되돌려 놓는다. render() 가 캐럿 위치만 복구하고 포커스는
        // 건드리지 않기 때문.
        editor.focus();
        return;
      }
      inFence = true;
    } else if (isFence && inFence) {
      inFence = false;
    }
  }
}

function renderTableRow(line, aligns, rowIdx, isSep) {
  // 진짜 `<tr>` + `<td>` 를 만들어 브라우저의 네이티브 표 모델을 그대로
  // 쓴다. 상·하 방향키 caret 이동, Cmd+C/V 복사·붙여넣기, 우클릭
  // context menu 가 모두 표준적으로 동작한다.
  //
  // 소스 문자열의 `|` 는 보존해야 하므로 각 `<td>` 안쪽에 숨김 `md-syn`
  // 으로 넣는다. 각 셀은 "앞 파이프 + 셀 본문" 을 담고, **마지막 셀만**
  // 트레일링 파이프를 추가로 담아 라인 전체 textContent == 소스 문자열.
  const cls = [
    'md-line', 'md-table-row',
    rowIdx === 0 ? 'md-table-head' : '',
    isSep ? 'md-table-sep' : '',
  ].filter(Boolean).join(' ');
  const row = document.createElement('tr');
  row.className = cls;
  const leadMatch = line.match(/^(\s*)/);
  const leadWs = leadMatch[1];
  const rest = line.slice(leadWs.length);
  const pipePos = [];
  for (let i = 0; i < rest.length; i++) if (rest[i] === '|') pipePos.push(i);
  // 표 행은 파이프가 최소 2개라고 가정(양 끝). 못 만나면 안전하게 한 셀로.
  if (pipePos.length < 2) {
    const td = document.createElement('td');
    td.className = 'md-cell';
    if (leadWs) td.appendChild(document.createTextNode(leadWs));
    td.appendChild(document.createTextNode(rest));
    row.appendChild(td);
    return row;
  }
  const numCells = pipePos.length - 1;
  for (let j = 0; j < numCells; j++) {
    const start = pipePos[j];
    const end = pipePos[j + 1];
    const content = rest.slice(start + 1, end);
    const td = document.createElement('td');
    td.className = 'md-cell';
    const a = aligns && aligns[j];
    if (a) td.dataset.align = a;
    if (isSep) td.classList.add('md-cell-sep', 'md-syn');
    // 맨 앞 셀에만 리딩 공백 포함(들여쓰인 표 지원).
    if (j === 0 && leadWs) td.appendChild(document.createTextNode(leadWs));
    // 앞 파이프(숨김).
    const p1 = document.createElement('span');
    p1.className = 'md-marker md-syn md-pipe';
    p1.textContent = '|';
    td.appendChild(p1);
    // 본문.
    if (isSep) {
      td.appendChild(document.createTextNode(content));
    } else {
      appendInline(td, content);
      // 빈 셀의 caret anchor — 0길이 text node 는 webkit 에서 layout 위치가
      // 모호해 caret 이 cell padding 바깥(td 좌측 가장자리) 에 그려지는 회귀
      // 가 있다. 0길이 text node + <br> 조합으로 대체:
      //   • 0길이 text node 는 lineInnerAt(SHOW_TEXT walker) 가 caret 복원 시
      //     찾을 수 있는 타깃 역할 (소스 길이 0 기여).
      //   • <br> 은 정의된 baseline 박스를 만들어 caret 이 padding 안쪽
      //     content-area-start 에 자연스럽게 안착하도록 보장 (textContent === '',
      //     소스 길이 0 기여).
      if (content === '') {
        td.appendChild(document.createTextNode(''));
        td.appendChild(document.createElement('br'));
      }
    }
    // 마지막 셀: 트레일링 파이프도 여기 담는다.
    if (j === numCells - 1) {
      const p2 = document.createElement('span');
      p2.className = 'md-marker md-syn md-pipe';
      p2.textContent = '|';
      td.appendChild(p2);
    }
    row.appendChild(td);
  }
  return row;
}

function makeLineBlock(cls) {
  const el = document.createElement('div');
  el.className = cls;
  return el;
}

function renderMdLineBlock(line) {
  // Empty line — keep a zero-width child so min-height still applies
  // and cursor can land inside it.
  if (line === '') {
    const block = makeLineBlock('md-line md-para');
    return block;
  }
  // Heading: 1 to 6 '#' then a space.
  let m = line.match(/^(#{1,6}) (.*)$/);
  if (m) {
    const lvl = m[1].length;
    const block = makeLineBlock(`md-line md-h${lvl}`);
    appendMarker(block, m[1] + ' ', { syn: true });
    appendInline(block, m[2]);
    return block;
  }
  // Blockquote.
  m = line.match(/^(> ?)(.*)$/);
  if (m) {
    const block = makeLineBlock('md-line md-quote');
    appendMarker(block, m[1], { syn: true });
    if (m[2]) {
      appendInline(block, m[2]);
    } else {
      // 빈 인용 — 캐럿이 앉을 수 있도록 BR 한 개 추가. textContent 0 이라
      // 소스 길이 계산에 영향 없음.
      block.appendChild(document.createElement('br'));
    }
    return block;
  }
  // Task list — render as a visible checkbox icon while keeping the
  // raw source (`- [x] `) in a hidden span so cursor math + editing
  // keep working. The icon is click-to-toggle.
  m = line.match(/^(\s*)([-*+]) \[([ xX])\] (.*)$/);
  if (m) {
    const indent = m[1];
    const box = m[3].toLowerCase() === 'x';
    const block = makeLineBlock('md-line md-list md-task' + (box ? ' md-task-done' : ''));
    block.dataset.indent = String(Math.min(4, Math.floor(indent.length / 2)));
    if (indent) block.appendChild(document.createTextNode(indent));
    // Hidden source span — keeps textContent in sync with `committed`.
    const src = document.createElement('span');
    src.className = 'md-marker md-syn md-task-src';
    src.textContent = `${m[2]} [${m[3]}] `;
    block.appendChild(src);
    // Visible checkbox glyph — not part of textContent (SVG has none).
    const chk = document.createElement('span');
    chk.className = 'md-task-check' + (box ? ' checked' : '');
    chk.contentEditable = 'false';
    chk.setAttribute('aria-label', box ? '완료됨' : '미완료');
    chk.innerHTML = box
      ? '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="2.2" y="2.2" width="11.6" height="11.6" rx="2.4"/><polyline points="5,8 7.2,10 11,6"/></svg>'
      : '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2.2" y="2.2" width="11.6" height="11.6" rx="2.4"/></svg>';
    block.appendChild(chk);
    // 체크박스는 contenteditable=false 라, 캐럿을 그 바로 뒤에 두려면
    // 뒤따르는 **editable inline 앵커** 가 필요하다. 없으면 브라우저가
    // 캐럿을 가장 가까운 편집 가능 위치(체크박스 앞쪽, md-task-src 경계)
    // 로 스냅시켜 커서가 왼쪽에 떠 보인다. 본문을 감싸는 body span 을
    // 항상 만들어서 캐럿이 앉을 자리(여기) 를 보장한다.
    const body = document.createElement('span');
    body.className = 'md-task-body';
    if (m[4]) appendInline(body, m[4]);
    block.appendChild(body);
    return block;
  }
  // Bullet list.
  m = line.match(/^(\s*)([-*+]) (.*)$/);
  if (m) {
    const indent = m[1];
    const block = makeLineBlock('md-line md-list');
    block.dataset.indent = String(Math.min(4, Math.floor(indent.length / 2)));
    if (indent) block.appendChild(document.createTextNode(indent));
    const ms = document.createElement('span');
    ms.className = 'md-marker md-bullet';
    ms.textContent = m[2] + ' ';
    block.appendChild(ms);
    appendInline(block, m[3]);
    return block;
  }
  // Ordered list.
  m = line.match(/^(\s*)(\d+[.)]) (.*)$/);
  if (m) {
    const indent = m[1];
    const block = makeLineBlock('md-line md-list md-ol');
    block.dataset.indent = String(Math.min(4, Math.floor(indent.length / 2)));
    if (indent) block.appendChild(document.createTextNode(indent));
    const ms = document.createElement('span');
    ms.className = 'md-marker';
    ms.textContent = m[2] + ' ';
    block.appendChild(ms);
    appendInline(block, m[3]);
    return block;
  }
  // Horizontal rule.
  if (/^\s*(?:-{3,}|_{3,}|\*{3,})\s*$/.test(line)) {
    const block = makeLineBlock('md-line md-hr');
    const ms = document.createElement('span');
    ms.className = 'md-marker md-syn';
    ms.textContent = line;
    block.appendChild(ms);
    return block;
  }
  // [TOC] — 자체는 일반 단락처럼 보이게 텍스트를 유지하되, render 이후
  // renderTocBlocks 가 이 라인 아래에 제목 목록 프리뷰를 붙인다.
  if (/^\s*\[TOC\]\s*$/i.test(line)) {
    const block = makeLineBlock('md-line md-toc');
    const ms = document.createElement('span');
    ms.className = 'md-marker md-syn';
    ms.textContent = line;
    block.appendChild(ms);
    return block;
  }
  // Plain paragraph.
  const block = makeLineBlock('md-line md-para');
  appendInline(block, line);
  return block;
}

function appendMarker(parent, text, opts) {
  if (!text) return;
  const s = document.createElement('span');
  s.className = 'md-marker' + (opts && opts.syn !== false ? ' md-syn' : '');
  s.textContent = text;
  parent.appendChild(s);
}

// Inline tokens the parser recognizes. Order matters: code first
// (atomic, no nested parsing), then images (before links — they share
// the `[...]` form but `![...]` is an image), then **wiki-links
// `[[Page]]`** (before regular `[text](url)` so the double-bracket form
// isn't eaten by the single-bracket pattern), then links, strong before
// em, strikethrough, autolinks.
const INLINE_RE =
  /(`[^`\n]+?`)|(!\[[^\]\n]*?\]\([^)\n]+?\))|(\[\[[^\]\n]+?\]\])|(\[[^\]\n]+?\]\([^)\n]+?\))|(\*\*[^*\n]+?\*\*)|(\*[^*\n]+?\*)|(~~[^~\n]+?~~)|(<https?:\/\/[^>\s]+>)|(<[^>@\s]+@[^>@\s]+\.[^>\s]+>)/g;

function appendInline(parent, text) {
  if (!text) return;
  INLINE_RE.lastIndex = 0;
  let pos = 0;
  let m;
  while ((m = INLINE_RE.exec(text))) {
    if (m.index > pos) {
      parent.appendChild(document.createTextNode(text.slice(pos, m.index)));
    }
    const tok = m[0];
    parent.appendChild(wrapInlineToken(tok));
    pos = m.index + tok.length;
  }
  if (pos < text.length) {
    parent.appendChild(document.createTextNode(text.slice(pos)));
  }
}

function wrapInlineToken(tok) {
  // Inline code: `code`
  if (tok.startsWith('`')) {
    const wrap = document.createElement('span');
    wrap.className = 'md-inline-code';
    appendMarker(wrap, '`', { syn: true });
    const code = document.createElement('code');
    code.textContent = tok.slice(1, -1);
    wrap.appendChild(code);
    appendMarker(wrap, '`', { syn: true });
    return wrap;
  }
  // Image: ![alt](url)
  if (tok.startsWith('![')) {
    const close = tok.indexOf('](');
    const alt = tok.slice(2, close);
    const url = tok.slice(close + 2, -1);
    const wrap = document.createElement('span');
    wrap.className = 'md-image';
    appendMarker(wrap, '![', { syn: true });
    if (alt) {
      const altSpan = document.createElement('span');
      altSpan.className = 'md-image-alt';
      altSpan.textContent = alt;
      wrap.appendChild(altSpan);
    }
    appendMarker(wrap, '](', { syn: true });
    const urlSpan = document.createElement('span');
    urlSpan.className = 'md-marker md-syn';
    urlSpan.textContent = url;
    wrap.appendChild(urlSpan);
    appendMarker(wrap, ')', { syn: true });
    // Actual rendered preview — not part of textContent because <img>
    // is a replaced element with no text.
    const img = document.createElement('img');
    img.src = url;
    img.alt = alt;
    img.loading = 'lazy';
    img.className = 'md-image-preview';
    img.addEventListener('error', () => img.classList.add('broken'));
    wrap.appendChild(img);
    return wrap;
  }
  // Strong: **text**
  if (tok.startsWith('**')) {
    const wrap = document.createElement('span');
    appendMarker(wrap, '**', { syn: true });
    const body = document.createElement('strong');
    body.textContent = tok.slice(2, -2);
    wrap.appendChild(body);
    appendMarker(wrap, '**', { syn: true });
    return wrap;
  }
  // Em: *text*
  if (tok.startsWith('*')) {
    const wrap = document.createElement('span');
    appendMarker(wrap, '*', { syn: true });
    const body = document.createElement('em');
    body.textContent = tok.slice(1, -1);
    wrap.appendChild(body);
    appendMarker(wrap, '*', { syn: true });
    return wrap;
  }
  // Strike: ~~text~~
  if (tok.startsWith('~~')) {
    const wrap = document.createElement('span');
    appendMarker(wrap, '~~', { syn: true });
    const body = document.createElement('s');
    body.textContent = tok.slice(2, -2);
    wrap.appendChild(body);
    appendMarker(wrap, '~~', { syn: true });
    return wrap;
  }
  // Wiki link: [[문서 이름]] — 내부 문서 링크. ⌘+클릭으로 열고, 매칭되는
  // .md 파일이 없으면 "만들까요?" 확인 후 생성.
  if (tok.startsWith('[[') && tok.endsWith(']]')) {
    const name = tok.slice(2, -2);
    const wrap = document.createElement('span');
    wrap.className = 'md-wikilink';
    wrap.dataset.name = name;
    appendMarker(wrap, '[[', { syn: true });
    const a = document.createElement('a');
    a.className = 'md-wikilink-body';
    a.textContent = name;
    a.href = '#';
    a.title = `⌘+클릭으로 열기 · [[${name}]]`;
    wrap.appendChild(a);
    appendMarker(wrap, ']]', { syn: true });
    markWikilinkExistence(wrap, name);
    return wrap;
  }
  // Link: [text](url)
  if (tok.startsWith('[')) {
    const closeBracket = tok.indexOf('](');
    const text = tok.slice(1, closeBracket);
    const url = tok.slice(closeBracket + 2, -1);
    const wrap = document.createElement('span');
    wrap.className = 'md-link';
    // 열기 `[` 는 한 개의 숨긴 마커. 닫는 쪽은 `](url)` 를 **단일 span**
    // 으로 묶어 여러 개의 인라인 박스 사이에 쌓이는 미세한 white-space·
    // letter-spacing 여백이 누적되는 문제를 차단한다.
    appendMarker(wrap, '[', { syn: true });
    const a = document.createElement('a');
    a.textContent = text;
    a.href = url;
    a.target = '_blank';
    a.rel = 'noopener noreferrer';
    a.title = `⌘+클릭으로 열기 · ${url}`;
    wrap.appendChild(a);
    appendMarker(wrap, `](${url})`, { syn: true });
    return wrap;
  }
  // Autolink: <url> or <email>
  if (tok.startsWith('<') && tok.endsWith('>')) {
    const inner = tok.slice(1, -1);
    const isMail = /@/.test(inner) && !/^https?:\/\//.test(inner);
    const wrap = document.createElement('span');
    wrap.className = 'md-link md-autolink';
    appendMarker(wrap, '<', { syn: true });
    const a = document.createElement('a');
    a.textContent = inner;
    a.href = isMail ? `mailto:${inner}` : inner;
    a.target = '_blank';
    a.rel = 'noopener noreferrer';
    a.title = `⌘+클릭으로 열기 · ${inner}`;
    wrap.appendChild(a);
    appendMarker(wrap, '>', { syn: true });
    return wrap;
  }
  return document.createTextNode(tok);
}

/* ─────────── Source-index ↔ DOM-position helpers ─────────── */
// `.md-code-lang-select` 처럼 contenteditable=false 로 꽂아 둔 UI 엘리먼트
// 는 에디터 DOM 에 같이 살지만 committed 소스에는 존재하지 않는 문자를
// textContent 로 노출한다 (예: 언어 드롭다운의 option 라벨 전체). 커서
// 인덱스 계산은 소스 기준이어야 하므로 이 서브트리는 모두 배제한다.
function isExcludedFromSource(el) {
  if (!el || el.nodeType !== Node.ELEMENT_NODE) return false;
  if (!el.classList) return false;
  if (el.classList.contains('md-code-lang-select')) return true;
  return false;
}
function isNodeInsideExcluded(node) {
  let n = node;
  while (n && n !== editor) {
    if (n.nodeType === Node.ELEMENT_NODE && isExcludedFromSource(n)) return n;
    n = n.parentNode;
  }
  return null;
}
function lineSourceLength(line) {
  let len = line.textContent.length;
  const excluded = line.querySelectorAll('.md-code-lang-select');
  for (const el of excluded) len -= el.textContent.length;
  return len;
}
function childSourceLength(child) {
  // 라인의 직속 자식 하나의 "소스 기여분" 길이. 배제 대상이거나 그
  // 내부면 0, 아니면 자식이 품은 배제 서브트리 길이만 빼고 돌려준다.
  if (!child) return 0;
  if (child.nodeType === Node.ELEMENT_NODE && isExcludedFromSource(child)) return 0;
  let len = child.textContent.length;
  if (child.querySelectorAll) {
    for (const el of child.querySelectorAll('.md-code-lang-select')) {
      len -= el.textContent.length;
    }
  }
  return len;
}

// In markdown mode each source line is its own <div class="md-line">,
// with no explicit \n text node between them (the block boundary is
// implicit). In plain mode \n becomes <br>. Both cases require walking
// DOM while accounting for the "virtual" newline between blocks/BRs.
function sourceIdxToDom(idx) {
  // Markdown path: walk .md-line blocks in document order. 표 행은 진짜
  // <tr> 이라 editor.children 에 직접 들어있지 않으므로 flat 스캔 대신
  // querySelectorAll 로 수집. 반환 순서 = DOM 문서 순서 = 소스 순서.
  if (markdownMode) {
    let acc = 0;
    const blocks = editor.querySelectorAll('.md-line');
    for (let i = 0; i < blocks.length; i++) {
      const b = blocks[i];
      const len = lineSourceLength(b);
      if (idx <= acc + len) return lineInnerAt(b, idx - acc);
      acc += len + 1;
    }
    // past end
    const last = blocks[blocks.length - 1];
    if (last) return lineInnerAt(last, lineSourceLength(last));
    return { node: editor, offset: 0 };
  }
  // Plain path: walk editor.childNodes; BR = 1 char; text node = its length.
  let acc = 0;
  let lastText = null;
  const walker = document.createTreeWalker(editor, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT, {
    acceptNode(n) {
      if (n.classList && n.classList.contains('caret-probe')) return NodeFilter.FILTER_REJECT;
      if (n.nodeType === Node.TEXT_NODE) return NodeFilter.FILTER_ACCEPT;
      if (n.nodeName === 'BR') return NodeFilter.FILTER_ACCEPT;
      return NodeFilter.FILTER_SKIP; // descend but don't yield
    },
  });
  let node;
  while ((node = walker.nextNode())) {
    if (node.nodeType === Node.TEXT_NODE) {
      const len = node.nodeValue.length;
      if (acc + len >= idx) return { node, offset: idx - acc };
      acc += len;
      lastText = node;
    } else {
      // BR — counts 1 for \n. Caret before BR = end of previous line.
      if (acc === idx) {
        return { node: node.parentNode, offset: Array.prototype.indexOf.call(node.parentNode.childNodes, node) };
      }
      acc += 1;
    }
  }
  if (lastText) return { node: lastText, offset: lastText.nodeValue.length };
  return { node: editor, offset: editor.childNodes.length };
}

function lineInnerAt(line, offsetInLine) {
  // SHOW_TEXT 만으로도 DOM 을 훑지만, contenteditable=false UI 서브트리
  // (`.md-code-lang-select` 등) 안의 텍스트는 소스에 해당하지 않으므로
  // walker 에서 걸러 준다. 또 .md-syn 안쪽으로 캐럿이 들어가면 브라우저가
  // 임의 방향으로 스냅해 cursor 변수와 어긋나므로, 그런 케이스에서는
  // 합류 지점을 syn 바로 뒤로 밀어 놓는다.
  const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT, {
    acceptNode(n) {
      if (isNodeInsideExcluded(n)) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  let acc = 0;
  let node;
  let lastText = null;
  while ((node = walker.nextNode())) {
    const len = node.nodeValue.length;
    if (acc + len >= offsetInLine) {
      const inOffset = offsetInLine - acc;
      const synAncestor = ancestorMdSyn(node, line);
      if (synAncestor) {
        // syn 범위 안 → syn 다음 형제부터 첫 번째 text node 로 스냅.
        const next = firstTextAfter(synAncestor, line);
        if (next) return { node: next, offset: 0 };
        // 빈 할 일: body span 안쪽 0번 — 체크박스 오른쪽 편집 앵커.
        const taskBody = line.querySelector('.md-task-body');
        if (taskBody) return { node: taskBody, offset: 0 };
        // 빈 인용/그 외 BR 로 마감된 라인 — BR 바로 **앞** 에 둬 현재 줄
        // 안쪽에 캐럿이 머물게 한다(BR 뒤에 두면 다음 줄로 내려감).
        const last = line.lastChild;
        if (last && last.nodeName === 'BR') {
          return { node: line, offset: line.childNodes.length - 1 };
        }
        return { node: line, offset: line.childNodes.length };
      }
      return { node, offset: inOffset };
    }
    acc += len;
    lastText = node;
  }
  if (lastText) return { node: lastText, offset: lastText.nodeValue.length };
  // Empty line — set into the block itself at offset 0.
  return { node: line, offset: 0 };
}

function ancestorMdSyn(node, root) {
  let n = node;
  while (n && n !== root) {
    if (n.classList && n.classList.contains('md-syn')) return n;
    n = n.parentNode;
  }
  return null;
}
function firstTextAfter(el, root) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(n) {
      if (isNodeInsideExcluded(n)) return NodeFilter.FILTER_REJECT;
      if (el.contains(n)) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  let cur;
  while ((cur = walker.nextNode())) {
    if (el.compareDocumentPosition(cur) & Node.DOCUMENT_POSITION_FOLLOWING) {
      return cur;
    }
  }
  return null;
}

function placeCaretAtSourceIdx(idx) {
  // Tag the line with `.has-caret` for any consumers that still read it
  // (e.g. future features). The class is a no-op for rendering now —
  // syntax markers always stay hidden — and hidden `.md-syn` spans are
  // contenteditable=false, so the browser snaps to the nearest legal
  // position rather than sticking the caret inside an invisible span.
  if (markdownMode) markLineWithCaret(idx);
  const pos = sourceIdxToDom(Math.max(0, Math.min(idx, committed.length + preedit.length)));
  if (!pos) return;
  const sel = window.getSelection();
  if (!sel) return;
  const range = document.createRange();
  try {
    range.setStart(pos.node, pos.offset);
    range.collapse(true);
    sel.removeAllRanges();
    sel.addRange(range);
  } catch (e) {
    // Fall back to end-of-contents.
    range.selectNodeContents(editor);
    range.collapse(false);
    sel.removeAllRanges();
    sel.addRange(range);
  }
}

// Tag the .md-line containing the given source index with `.has-caret`
// so hidden syntax markers inside it become visible (caret-line focus
// reveal). Called from placeCaretAtSourceIdx after a render + from
// user-driven cursor moves (click / arrows).
function markLineWithCaret(idx) {
  editor.querySelectorAll('.md-line.has-caret').forEach((el) => el.classList.remove('has-caret'));
  let acc = 0;
  const blocks = editor.querySelectorAll('.md-line');
  for (let i = 0; i < blocks.length; i++) {
    const b = blocks[i];
    const len = lineSourceLength(b);
    if (idx <= acc + len) { b.classList.add('has-caret'); return; }
    acc += len + 1;
  }
  const last = blocks[blocks.length - 1];
  if (last) last.classList.add('has-caret');
}

function updateCaretLineFromSelection() {
  if (!markdownMode) return;
  const sel = window.getSelection();
  if (!sel || !sel.anchorNode || !editor.contains(sel.anchorNode)) return;
  editor.querySelectorAll('.md-line.has-caret').forEach((el) => el.classList.remove('has-caret'));
  let n = sel.anchorNode;
  while (n && n !== editor) {
    if (n.classList && n.classList.contains('md-line')) {
      n.classList.add('has-caret');
      // 커서가 .md-code-line 에서 빠져나갔으면 건너뛴 하이라이팅을
      // 따라잡아 칠해 준다. highlightCodeBlocks 는 대상 라인이 없으면
      // 즉시 반환하므로 가벼움. rAF 로 코알레스해서 selectionchange 연속
      // 발화 때도 프레임당 한 번만 돌게 한다.
      scheduleHighlightCatchup();
      return;
    }
    n = n.parentNode;
  }
}

let highlightCatchupScheduled = false;
function scheduleHighlightCatchup() {
  if (highlightCatchupScheduled) return;
  highlightCatchupScheduled = true;
  requestAnimationFrame(() => {
    highlightCatchupScheduled = false;
    highlightCodeBlocks();
  });
}

// Reverse mapping: DOM (node, offset) → source char index.
function domToSourceIdx(node, offset) {
  if (markdownMode) {
    // Find the .md-line ancestor of node (or node itself if it's a line).
    let line = node;
    while (line && line !== editor) {
      if (line.classList && line.classList.contains('md-line')) break;
      line = line.parentNode;
    }
    if (!line || line === editor) {
      // Offset is a child index of the editor — sum text-content lengths of
      // all preceding md-lines (flat, includes <tr> rows inside tables).
      let acc = 0;
      const blocks = editor.querySelectorAll('.md-line');
      const limit = (node === editor) ? Math.min(offset, blocks.length) : blocks.length;
      for (let i = 0; i < limit; i++) {
        acc += lineSourceLength(blocks[i]) + 1;
      }
      return Math.max(0, acc - 1);
    }
    // 선택이 배제 서브트리(언어 드롭다운 등) 안이면 그 라인 시작으로 스냅.
    if (isNodeInsideExcluded(node)) {
      let acc = 0;
      for (const b of editor.querySelectorAll('.md-line')) {
        if (b === line) break;
        acc += lineSourceLength(b) + 1;
      }
      return acc;
    }
    // Accumulate lines before this one.
    let acc = 0;
    for (const b of editor.querySelectorAll('.md-line')) {
      if (b === line) break;
      acc += lineSourceLength(b) + 1;
    }
    // Offset within the line. The selection's `node` may be a text node
    // (offset = within-text position) OR an element node (offset = child
    // index). A SHOW_TEXT-only walker misses the element case and falls
    // through to end-of-line, which is why clicks on image previews,
    // zero-width markers, or other inline atoms looked like "the caret
    // jumped to a different line".
    if (node === line) {
      // 표 행(tr.md-table-row) 자체에 caret 이 떨어진 케이스 — 보통 셀
      // 사이 경계 클릭 시 발생한다. childSourceLength 단순 합은 "이 셀의
      // leading pipe 직전" 위치를 주는데, 그러면 사용자가 입력 시 표 행의
      // | 보다 앞에 글자가 끼어 행 포맷이 깨진다. offset 만큼 진행한 셀의
      // leading pipe 직후로 스냅해 글자가 셀 안에 정확히 들어가게 한다.
      if (line.classList && line.classList.contains('md-table-row')) {
        const tds = [];
        for (const c of line.children) {
          if (c.classList && c.classList.contains('md-cell')) tds.push(c);
        }
        let withinLine = 0;
        const targetIdx = Math.max(0, Math.min(offset, tds.length));
        // offset 만큼의 앞쪽 셀 텍스트 누적
        for (let i = 0; i < targetIdx; i++) withinLine += childSourceLength(tds[i]);
        // 마지막 셀 너머는 그대로 라인 끝, 아니면 해당 셀의 leading pipe 까지 추가.
        if (targetIdx < tds.length) {
          for (const child of tds[targetIdx].childNodes) {
            withinLine += childSourceLength(child);
            if (child.nodeType === Node.ELEMENT_NODE
                && child.classList && child.classList.contains('md-pipe')) {
              break;
            }
          }
        }
        return acc + withinLine;
      }
      let inLine = 0;
      for (let i = 0; i < Math.min(offset, line.childNodes.length); i++) {
        inLine += childSourceLength(line.childNodes[i]);
      }
      return acc + inLine;
    }
    const walker = document.createTreeWalker(line, NodeFilter.SHOW_ALL, {
      acceptNode(n) {
        if (isNodeInsideExcluded(n)) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    let inLine = 0;
    let cur;
    while ((cur = walker.nextNode())) {
      if (cur === node) {
        if (cur.nodeType === Node.TEXT_NODE) {
          return acc + inLine + offset;
        }
        // 빈 표 셀(td.md-cell) 의 element-level 캐럿 스냅.
        // body 셀이 비어 있으면 td 의 모든 자식이 display:none 인 .md-pipe
        // 또는 0길이 텍스트라, 셀을 클릭했을 때 브라우저가 td 자체에 caret
        // 을 두는 경우가 있다. 일반 element 분기는 offset 0 → 라인 시작
        // (= 첫 파이프 앞) 으로 매핑돼, 사용자가 입력하면 행의 leading | 보다
        // 앞에 글자가 끼어 표 구조가 깨진다. 셀 안쪽(leading pipe 직후) 으로
        // 스냅해 입력이 셀 안에 정확히 들어가도록.
        if (cur.nodeType === Node.ELEMENT_NODE
            && cur.classList && cur.classList.contains('md-cell')) {
          let withinLine = 0;
          // 같은 행의 앞쪽 셀들의 소스 기여분 합.
          let sib = cur.previousElementSibling;
          while (sib && sib.classList && sib.classList.contains('md-cell')) {
            withinLine = childSourceLength(sib) + withinLine;
            sib = sib.previousElementSibling;
          }
          // 이 셀의 leading 부분(필요시 들여쓰기 텍스트 + leading .md-pipe) 까지 누적.
          for (const child of cur.childNodes) {
            withinLine += childSourceLength(child);
            if (child.nodeType === Node.ELEMENT_NODE
                && child.classList && child.classList.contains('md-pipe')) {
              break; // leading pipe 직후가 셀 안쪽 시작점
            }
          }
          return acc + withinLine;
        }
        // Element node — `offset` is a child index, so sum text lengths
        // of children up to that index. We stop the walker here; we do
        // NOT descend into `cur`'s subtree (counting it via childNodes
        // avoids double-counting).
        let inner = 0;
        for (let i = 0; i < Math.min(offset, cur.childNodes.length); i++) {
          inner += childSourceLength(cur.childNodes[i]);
        }
        return acc + inLine + inner;
      }
      if (cur.nodeType === Node.TEXT_NODE) {
        inLine += cur.nodeValue.length;
      }
    }
    return acc + Math.min(inLine, lineSourceLength(line));
  }
  // Plain mode.
  let acc = 0;
  const walker = document.createTreeWalker(editor, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT, null);
  let n;
  while ((n = walker.nextNode())) {
    if (n === node) {
      if (n.nodeType === Node.TEXT_NODE) return acc + offset;
      // Element container — offset is a child index.
      let local = 0;
      for (let i = 0; i < Math.min(offset, n.childNodes.length); i++) {
        local += measureDomText(n.childNodes[i]);
      }
      return acc + local;
    }
    if (n.classList && n.classList.contains('preedit')) {
      // We want source index; treat preedit as not part of committed.
      // However, cursor can point to preedit.length past cursor.
      // Skip subtree.
      acc += 0; // we don't add preedit to source
      // advance walker past subtree
      // NOTE: TreeWalker can't skip subtrees mid-iteration simply;
      // the preedit shouldn't contain selection anchors anyway.
      continue;
    }
    if (n.nodeType === Node.TEXT_NODE) acc += n.nodeValue.length;
    else if (n.nodeName === 'BR') acc += 1;
  }
  return acc;
}

function measureDomText(n) {
  if (!n) return 0;
  if (n.nodeType === Node.TEXT_NODE) return n.nodeValue.length;
  if (n.nodeName === 'BR') return 1;
  if (n.classList && n.classList.contains('preedit')) return 0;
  let t = 0;
  for (const c of n.childNodes) t += measureDomText(c);
  return t;
}

function insertPreeditAtSourceIdx(root, idx, text) {
  // 표 행은 진짜 <tr class="md-line.md-table-row"> 라 root 의 직접 자식이
  // 아니라 <table><tbody> 손자다. root.children 만 순회하면 표 행이 모두
  // 누락돼 preedit 이 표 바깥(루트 끝) 에 떨어지므로, querySelectorAll 로
  // 모든 .md-line 을 문서 순서로 모은다. 일반 라인(.md-line div) 과 표 행
  // (.md-line tr) 모두 leaf md-line 이라 중복 매치는 없다.
  const blocks = root.querySelectorAll('.md-line');
  let acc = 0;
  for (let i = 0; i < blocks.length; i++) {
    const b = blocks[i];
    const len = lineSourceLength(b);
    if (idx <= acc + len) {
      insertPreeditInBlock(b, idx - acc, text);
      return;
    }
    acc += len + 1;
  }
  // Past end — append to last block (있으면 그 안에, 없으면 root 에).
  const last = blocks.length ? blocks[blocks.length - 1] : null;
  const pe = makePreedit(text);
  if (last) last.appendChild(pe);
  else root.appendChild(pe);
}

function insertPreeditInBlock(block, offsetInBlock, text) {
  const walker = document.createTreeWalker(block, NodeFilter.SHOW_TEXT, {
    acceptNode(n) {
      if (isNodeInsideExcluded(n)) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  let acc = 0;
  let node;
  while ((node = walker.nextNode())) {
    const len = node.nodeValue.length;
    if (acc + len >= offsetInBlock) {
      const rest = node.splitText(offsetInBlock - acc);
      const pe = makePreedit(text);
      rest.parentNode.insertBefore(pe, rest);
      return;
    }
    acc += len;
  }
  block.appendChild(makePreedit(text));
}

function makePreedit(text) {
  const span = document.createElement('span');
  span.className = 'preedit';
  span.textContent = text;
  return span;
}

/* ─────────── Block image caret-stop (Notion-style) ───────────
 * A rendered `.md-image-preview` occupies visual space but isn't a
 * source character, so arrow-key navigation around it feels weird —
 * the caret either jumps past it or lands on an empty-looking line
 * before/after it. Treat it as an intermediate "caret stop": first
 * arrow toward it selects the image; a second press moves the caret
 * past the image's source line entirely. */
let selectedImage = null;

function selectImage(img) {
  if (!img) return;
  if (selectedImage && selectedImage !== img) selectedImage.classList.remove('md-image-selected');
  selectedImage = img;
  img.classList.add('md-image-selected');
}

function deselectImage() {
  if (!selectedImage) return;
  selectedImage.classList.remove('md-image-selected');
  selectedImage = null;
}

function caretLineEl() {
  const sel = window.getSelection();
  if (!sel || !sel.anchorNode || !editor.contains(sel.anchorNode)) return null;
  let n = sel.anchorNode;
  while (n && n !== editor) {
    if (n.classList && n.classList.contains('md-line')) return n;
    n = n.parentNode;
  }
  return null;
}

function imageLineEl(img) {
  let n = img;
  while (n && n !== editor) {
    if (n.classList && n.classList.contains('md-line')) return n;
    n = n.parentNode;
  }
  return null;
}

function prevMdLine(line) {
  let s = line && line.previousElementSibling;
  while (s && !(s.classList && s.classList.contains('md-line'))) s = s.previousElementSibling;
  return s;
}
function nextMdLine(line) {
  let s = line && line.nextElementSibling;
  while (s && !(s.classList && s.classList.contains('md-line'))) s = s.nextElementSibling;
  return s;
}

function sourceIdxAtLineStart(line) {
  let acc = 0;
  for (const b of editor.querySelectorAll('.md-line')) {
    if (b === line) return acc;
    acc += lineSourceLength(b) + 1;
  }
  return acc;
}

// Arrow-up pressed in markdown mode: returns true if the image
// caret-stop handled the event and default navigation should be skipped.
function tryImageCaretStopUp() {
  if (selectedImage) {
    const imgLine = imageLineEl(selectedImage);
    deselectImage();
    if (!imgLine) return false;
    const prev = prevMdLine(imgLine);
    cursor = prev ? sourceIdxAtLineStart(prev) + lineSourceLength(prev) : 0;
    placeCaretAtSourceIdx(cursor);
    ensureCaretVisible();
    return true;
  }
  const cur = caretLineEl();
  if (!cur) return false;
  const prev = prevMdLine(cur);
  if (!prev) return false;
  const img = prev.querySelector('.md-image-preview');
  if (!img || img.classList.contains('broken')) return false;
  selectImage(img);
  img.scrollIntoView({ block: 'nearest' });
  return true;
}

function tryImageCaretStopDown() {
  if (selectedImage) {
    const imgLine = imageLineEl(selectedImage);
    deselectImage();
    if (!imgLine) return false;
    const next = nextMdLine(imgLine);
    cursor = next ? sourceIdxAtLineStart(next) : committed.length;
    placeCaretAtSourceIdx(cursor);
    ensureCaretVisible();
    return true;
  }
  const cur = caretLineEl();
  if (!cur) return false;
  const next = nextMdLine(cur);
  if (!next) return false;
  const img = next.querySelector('.md-image-preview');
  if (!img || img.classList.contains('broken')) return false;
  selectImage(img);
  img.scrollIntoView({ block: 'nearest' });
  return true;
}

// 표 셀 간 위/아래 방향키 네비게이션. caret 이 td.md-cell 안에 있으면
// 같은 column index 의 인접 행 셀로 이동하고 cursor 변수도 동기화한다.
// 인접 행이 .md-table-sep(구분선, display:none) 이면 그 다음 행을 본다.
// caret 이 표 안에 없거나 인접 행이 없으면 false 를 반환해 호출자가
// 브라우저 기본 동작에 맡기도록 한다.
function tryTableArrowNav(direction) {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0 || !sel.isCollapsed) return false;
  const r = sel.getRangeAt(0);
  if (!editor.contains(r.startContainer)) return false;
  // td.md-cell 조상 찾기.
  let cell = r.startContainer;
  if (cell.nodeType === Node.TEXT_NODE) cell = cell.parentNode;
  while (cell && cell !== editor) {
    if (cell.nodeType === Node.ELEMENT_NODE
        && cell.classList && cell.classList.contains('md-cell')) break;
    cell = cell.parentNode;
  }
  if (!cell || cell === editor) return false;
  const tr = cell.parentNode;
  if (!tr || !tr.classList || !tr.classList.contains('md-table-row')) return false;
  // 같은 column index.
  const colIdx = Array.prototype.indexOf.call(tr.children, cell);
  if (colIdx < 0) return false;
  // 인접 행 (구분선 건너뜀).
  let target = direction === 'up' ? tr.previousElementSibling : tr.nextElementSibling;
  while (target && target.classList && target.classList.contains('md-table-sep')) {
    target = direction === 'up' ? target.previousElementSibling : target.nextElementSibling;
  }
  if (!target || !target.classList || !target.classList.contains('md-table-row')) {
    return false; // 표 위/아래 끝 — 브라우저가 표 밖으로 caret 을 빼낸다.
  }
  const targetCell = target.children[colIdx];
  if (!targetCell || !targetCell.classList || !targetCell.classList.contains('md-cell')) {
    return false;
  }
  // 가능하면 같은 x 좌표 부근으로 caret 정렬, 폴백은 타깃 셀 끝.
  const range = document.createRange();
  let placed = false;
  const cr = caretRect();
  if (cr && document.caretRangeFromPoint) {
    const tRect = targetCell.getBoundingClientRect();
    const x = Math.max(tRect.left + 1, Math.min(cr.left, tRect.right - 1));
    const y = tRect.top + tRect.height / 2;
    const fromPoint = document.caretRangeFromPoint(x, y);
    if (fromPoint && targetCell.contains(fromPoint.startContainer)) {
      range.setStart(fromPoint.startContainer, fromPoint.startOffset);
      placed = true;
    }
  }
  if (!placed) {
    // 셀의 마지막 텍스트 노드 끝 (없으면 셀 자체의 끝).
    const walker = document.createTreeWalker(targetCell, NodeFilter.SHOW_TEXT, {
      acceptNode(n) {
        if (isNodeInsideExcluded(n)) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    let lastText = null;
    let n;
    while ((n = walker.nextNode())) lastText = n;
    if (lastText) {
      range.setStart(lastText, lastText.nodeValue.length);
    } else {
      range.setStart(targetCell, targetCell.childNodes.length);
    }
  }
  range.collapse(true);
  sel.removeAllRanges();
  sel.addRange(range);
  // cursor 변수 동기화.
  const idx = domToSourceIdx(range.startContainer, range.startOffset);
  if (typeof idx === 'number') cursor = idx;
  ensureCaretVisible();
  return true;
}

/**
 * Returns a ClientRect for the caret. Handles the 0×0 collapsed-range
 * case by briefly inserting a zero-width probe at the selection point,
 * measuring, then removing it.
 */
function caretRect() {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0).cloneRange();
  let rect = range.getBoundingClientRect();
  if (rect.width || rect.height) return rect;
  // Zero-sized range (empty line, caret just before a <br>, etc.) \u2014
  // insert a zero-width probe to measure. `insertNode` splits the text
  // node at the range's position, which invalidates the user's current
  // selection. We MUST restore the caret afterwards; otherwise the
  // selection silently jumps to wherever the browser reseats it (which
  // used to be "end of editor", sending the visible caret to the last
  // line on every click that landed on a zero-rect position).
  const probe = document.createElement('span');
  probe.className = 'caret-probe';
  probe.appendChild(document.createTextNode('\u200b'));
  range.insertNode(probe);
  rect = probe.getBoundingClientRect();
  probe.remove();
  // Re-anchor the caret at `cursor` (+ preedit) using our own DOM
  // mapper, so the visible caret stays exactly where the user clicked
  // / where the render left it.
  try {
    placeCaretAtSourceIdx(cursor + (preedit ? preedit.length : 0));
  } catch (_) {
    // Fall back to leaving whatever selection the browser settled on;
    // correct visual state will be restored on the next render.
  }
  return rect;
}

async function updateStatus() {
  const info = await invoke('current_layout');
  currentHangulLayoutId = info.hangul_layout_id;
  const matchSel = (sel, value) => {
    if (sel.value !== value) sel.value = value;
  };
  matchSel(layoutSelect, info.hangul_layout_id);
  matchSel(latinSelect, info.latin_layout_id);
  matchSel(formSelect, info.output_form);
  if (info.backspace_mode) matchSel(bsModeSelect, info.backspace_mode);
  moachigiToggle.checked = info.compose_mode === 'moachigi';
  const osIme = isOsImeMode();
  if (info.supports_moachigi && !osIme) {
    modeLabel.classList.remove('disabled');
    moachigiToggle.disabled = false;
  } else {
    modeLabel.classList.add('disabled');
    moachigiToggle.disabled = true;
  }
  abbrToggle.checked = info.suggestions_enabled !== false;
  // In OS IME mode, abbreviation suggestions + language toggle are moot.
  abbrToggle.disabled = osIme;
  langToggle.disabled = osIme;
  langToggle.classList.toggle('disabled', osIme);
  // Language toggle visuals
  if (info.input_mode === 'english') {
    langCurrentEl.textContent = 'EN';
    langOtherEl.textContent = '한';
  } else {
    langCurrentEl.textContent = '한';
    langOtherEl.textContent = 'EN';
  }
  // Status bar layout indicator — "세벌식 최종 · 모아치기" / "두벌식 표준 · 순차"
  const layoutEl = document.getElementById('status-layout');
  const composeEl = document.getElementById('status-compose');
  if (layoutEl) {
    const name = hangulLayoutLabel(info.hangul_layout_id);
    layoutEl.textContent = name;
    layoutEl.dataset.layout = layoutKindTag(info.hangul_layout_id);
  }
  if (composeEl) {
    // Only surface compose state for layouts that support moachigi
    // (세벌식). 두벌식은 항상 Sequential 이므로 표시 생략.
    if (info.supports_moachigi) {
      const is_moa = info.compose_mode === 'moachigi';
      composeEl.hidden = false;
      composeEl.textContent = is_moa ? '모아치기' : '순차';
      composeEl.classList.toggle('active', is_moa);
      composeEl.title = is_moa
        ? '세벌식 모아치기 — 초·중·종 순서 무관'
        : '순차 조합 — 키 입력 순서대로 합성';
    } else {
      composeEl.hidden = true;
    }
  }
}

function hangulLayoutLabel(id) {
  switch (id) {
    case 'dubeolsik-std': return '두벌식 표준';
    case 'sebeolsik-390': return '세벌식 390';
    case 'sebeolsik-final': return '세벌식 최종';
    case 'os-ime': return 'OS IME';
    default: return id || '—';
  }
}

function layoutKindTag(id) {
  if (id === 'dubeolsik-std') return 'dubeol';
  if (id === 'sebeolsik-390' || id === 'sebeolsik-final') return 'sebeol';
  if (id === 'os-ime') return 'os';
  return 'other';
}

/* ─────────── Status pill quick pickers ─────────── */
function renderPickerPopover(anchorEl, title, options, currentValue, onPick) {
  // Reuse a single popover element; close any existing first.
  let pop = document.getElementById('status-picker');
  if (pop) pop.remove();
  pop = document.createElement('div');
  pop.id = 'status-picker';
  pop.className = 'status-picker';
  pop.setAttribute('role', 'menu');

  if (title) {
    const h = document.createElement('div');
    h.className = 'sp-head';
    h.textContent = title;
    pop.appendChild(h);
  }
  for (const [val, label] of options) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.textContent = label;
    if (val === currentValue) btn.classList.add('current');
    btn.addEventListener('click', async (ev) => {
      ev.stopPropagation();
      pop.remove();
      try { await onPick(val); } catch (e) { logEvent(String(e)); }
    });
    pop.appendChild(btn);
  }
  document.body.appendChild(pop);
  // Position above the anchor (status bar is at the bottom).
  const ar = anchorEl.getBoundingClientRect();
  const pr = pop.getBoundingClientRect();
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  let top = ar.top - pr.height - 8;
  if (top < 8) top = ar.bottom + 8; // flip below if not enough space above
  let left = ar.left + (ar.width / 2) - (pr.width / 2);
  left = Math.max(8, Math.min(left, vw - pr.width - 8));
  top = Math.max(8, Math.min(top, vh - pr.height - 8));
  pop.style.top = `${top}px`;
  pop.style.left = `${left}px`;

  // Dismiss on outside click / Escape.
  const onDocClick = (ev) => {
    if (!ev.target.closest('#status-picker')) close();
  };
  const onEsc = (ev) => { if (ev.key === 'Escape') close(); };
  function close() {
    if (pop && pop.parentNode) pop.remove();
    document.removeEventListener('click', onDocClick, true);
    window.removeEventListener('keydown', onEsc, true);
  }
  setTimeout(() => {
    document.addEventListener('click', onDocClick, true);
    window.addEventListener('keydown', onEsc, true);
  }, 0);
}

async function openLayoutPicker(ev) {
  const anchor = ev.currentTarget;
  const options = [
    ['dubeolsik-std', '두벌식 표준'],
    ['sebeolsik-390', '세벌식 390'],
    ['sebeolsik-final', '세벌식 최종'],
    ['os-ime', 'OS IME (시스템)'],
  ];
  renderPickerPopover(anchor, '한글 자판', options, currentHangulLayoutId, async (id) => {
    // Mirror layoutSelect's change flow so open-composition + OS-IME
    // transitions behave identically.
    const wasOsIme = isOsImeMode();
    if (wasOsIme) {
      committed = (editor.innerText || editor.textContent || '').replace(/\r\n/g, '\n');
      preedit = '';
      cursor = committed.length;
    }
    const flushed = await invoke('flush');
    if (flushed && flushed.commit) {
      committed = committed.slice(0, cursor) + flushed.commit + committed.slice(cursor);
      cursor += flushed.commit.length;
    }
    preedit = '';
    const info = await invoke('set_layout', { id });
    currentHangulLayoutId = info.hangul_layout_id;
    if (layoutSelect) layoutSelect.value = id;
    renderCore();
    if (isOsImeMode()) {
      cancelSuggestionDebounce();
      hideSuggestions();
      suggestionItems = [];
    }
    await renderKeyboardMap();
    await updateStatus();
    logEvent(`자판 → ${info.name}`);
  });
}

async function openComposePicker(ev) {
  if (moachigiToggle && moachigiToggle.disabled) return; // 두벌식·OS IME 는 선택 불가
  const anchor = ev.currentTarget;
  const cur = moachigiToggle && moachigiToggle.checked ? 'moachigi' : 'sequential';
  const options = [
    ['moachigi', '모아치기 — 초·중·종 순서 무관'],
    ['sequential', '순차 — 키 순서대로 조합'],
  ];
  renderPickerPopover(anchor, '조합 방식', options, cur, async (mode) => {
    const flushed = await invoke('flush');
    if (flushed && flushed.commit) {
      committed = committed.slice(0, cursor) + flushed.commit + committed.slice(cursor);
      cursor += flushed.commit.length;
    }
    preedit = '';
    await invoke('set_compose_mode', { mode });
    if (moachigiToggle) moachigiToggle.checked = (mode === 'moachigi');
    renderCore();
    await updateStatus();
    logEvent(`조합 → ${mode === 'moachigi' ? '모아치기' : '순차'}`);
  });
}

document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('status-layout')?.addEventListener('click', openLayoutPicker);
  document.getElementById('status-compose')?.addEventListener('click', openComposePicker);
});

async function applyResponse(resp, { fallbackChar } = {}) {
  // Snapshot for undo IF this response will actually change text.
  // Coalesce rapid keystrokes so a burst counts as one undo step.
  const willMutate = (resp.rollback_chars && resp.rollback_chars > 0)
    || !!resp.commit
    || !!resp.emitted_char
    || (resp.passthrough && fallbackChar)
    || (resp.preedit !== undefined && resp.preedit !== preedit);
  if (willMutate) snapshot({ coalesce: true });

  // Rollback first (abbreviation expansion consumed the typed trigger).
  if (resp.rollback_chars && resp.rollback_chars > 0) {
    const before = Array.from(committed.slice(0, cursor));
    const keep = Math.max(0, before.length - resp.rollback_chars);
    const newBefore = before.slice(0, keep).join('');
    committed = newBefore + committed.slice(cursor);
    cursor = newBefore.length;
  }
  let appended = '';
  if (resp.commit) appended += resp.commit;
  if (resp.emitted_char) appended += resp.emitted_char;
  if (resp.passthrough && fallbackChar && !resp.emitted_char) {
    appended += fallbackChar;
  }
  if (appended) {
    committed = committed.slice(0, cursor) + appended + committed.slice(cursor);
    cursor += appended.length;
    // 오탈자 교정: 방금 커밋된 끝이 완성 음절이고 바로 앞이 홀자모면 지움.
    const removed = typoCorrectAfterCommit();
    if (removed > 0) cursor -= removed;
  }
  preedit = resp.preedit || '';
  // Language mode may have flipped (Shift+Space/Caps). Sync the indicator.
  if (resp.input_mode) {
    applyInputMode(resp.input_mode);
  }
  const parts = [];
  if (resp.abbr_fired) parts.push(`🪄 ${resp.abbr_fired}`);
  if (resp.commit) parts.push(`commit="${resp.commit}"`);
  if (resp.preedit) parts.push(`preedit="${resp.preedit}"`);
  if (resp.passthrough) parts.push('[pass]');
  if (resp.rollback_chars) parts.push(`rollback=${resp.rollback_chars}`);
  if (resp.emitted_char) parts.push(`char=${JSON.stringify(resp.emitted_char)}`);
  lastEventEl.textContent = parts.join(' ') || 'no-op';
  fsmStateEl.textContent = preedit ? 'Composing' : 'Empty';
  // Render first — creates a fresh caret marker at the new end
  // position. `updateSuggestions` anchors the popup to it.
  render();
  // 오탈자 교정이 발생했다면 캐럿 부근에서 미세한 시각 피드백을 한 번
  // 보여 준다. render() 후에 selection 이 새 위치에 있어야 좌표가 맞다.
  if (lastTypoCorrection) {
    const c = lastTypoCorrection;
    lastTypoCorrection = null;
    requestAnimationFrame(() => flashTypoCorrection(c.kind));
  }
  updateSuggestions(resp.suggestions);
}

// Debounce timer for the rendered popup. Rapid typing cancels the
// pending render and schedules a new one, so the popup only "settles"
// when the user pauses briefly. Empty / toggle-off states take effect
// immediately to avoid stale content.
let suggestionDebounceTimer = null;
const SUGGESTION_DEBOUNCE_MS = 90;

function updateSuggestions(items) {
  // Hard off-switch: the toggle in the header suppresses the popup
  // regardless of what the backend sends.
  if (abbrToggle && !abbrToggle.checked) {
    cancelSuggestionDebounce();
    hideSuggestions();
    suggestionItems = [];
    return;
  }
  const next = Array.isArray(items) ? items : [];
  if (next.length === 0 || suggestionsManuallyDismissed) {
    cancelSuggestionDebounce();
    suggestionItems = [];
    hideSuggestions();
    return;
  }
  suggestionItems = next;
  suggestionIndex = Math.min(suggestionIndex, suggestionItems.length - 1);
  if (suggestionIndex < 0) suggestionIndex = 0;

  // If the popup is already visible, re-render synchronously so the
  // already-visible list stays correct. Only the *opening* of the
  // popup is debounced to keep quick typing from flashing.
  if (!suggestionsEl.classList.contains('hidden')) {
    renderSuggestionsAndPosition();
    return;
  }
  cancelSuggestionDebounce();
  suggestionDebounceTimer = setTimeout(() => {
    suggestionDebounceTimer = null;
    if (suggestionItems.length === 0 || suggestionsManuallyDismissed) return;
    if (abbrToggle && !abbrToggle.checked) return;
    renderSuggestionsAndPosition();
  }, SUGGESTION_DEBOUNCE_MS);
}

function cancelSuggestionDebounce() {
  if (suggestionDebounceTimer) {
    clearTimeout(suggestionDebounceTimer);
    suggestionDebounceTimer = null;
  }
}

function renderSuggestionsAndPosition() {
  renderSuggestions();
  suggestionsEl.style.visibility = 'hidden';
  suggestionsEl.classList.remove('hidden');
  positionSuggestions();
  suggestionsEl.style.visibility = '';
}

function renderSuggestions() {
  suggestionsListEl.innerHTML = '';
  suggestionItems.forEach((item, i) => {
    const li = document.createElement('li');
    li.className = 'suggestion-item' + (i === suggestionIndex ? ' active' : '');
    li.setAttribute('role', 'option');
    li.setAttribute('aria-selected', i === suggestionIndex ? 'true' : 'false');
    li.addEventListener('mousedown', (e) => {
      e.preventDefault();
      suggestionIndex = i;
      acceptSuggestion();
    });
    const idx = document.createElement('span');
    idx.className = 'suggestion-index';
    idx.textContent = (i + 1).toString();
    // Highlight the part of the trigger that matches the typed tail.
    const trig = document.createElement('span');
    trig.className = 'suggestion-trigger' + (item.is_exact ? ' exact' : '');
    const triggerChars = Array.from(item.trigger);
    const ms = item.match_start || 0;
    const mlen = Math.max(0, item.rollback_chars || 0);
    if (!item.is_exact && mlen > 0 && mlen < triggerChars.length) {
      const before = triggerChars.slice(0, ms).join('');
      const hit = triggerChars.slice(ms, ms + mlen).join('');
      const after = triggerChars.slice(ms + mlen).join('');
      if (before) trig.appendChild(document.createTextNode(before));
      const hi = document.createElement('b');
      hi.className = 'match-highlight';
      hi.textContent = hit;
      trig.appendChild(hi);
      if (after) trig.appendChild(document.createTextNode(after));
    } else {
      trig.textContent = item.trigger;
    }
    const body = document.createElement('span');
    body.className = 'suggestion-body';
    body.textContent = item.body.replace(/\n/g, ' ⏎ ');
    body.title = item.body;
    li.appendChild(idx);
    li.appendChild(trig);
    li.appendChild(body);
    // Match-kind badge: exact / prefix / 부분 (substring).
    const badge = document.createElement('span');
    if (item.is_exact) {
      badge.className = 'suggestion-badge exact';
      badge.textContent = 'exact';
    } else if (item.is_prefix) {
      badge.className = 'suggestion-badge prefix';
      badge.textContent = '접두';
    } else {
      badge.className = 'suggestion-badge substring';
      badge.textContent = '부분';
    }
    li.appendChild(badge);
    suggestionsListEl.appendChild(li);
  });
}

function positionSuggestions() {
  // Anchor such that the popup NEVER covers the caret's line box,
  // regardless of how cramped the viewport is. Strategy:
  //   1. Compute the available vertical space above and below the
  //      caret, clipped by floating overlays (tab bar / style bar /
  //      status bar) — those eat into the usable viewport.
  //   2. Place below if it fits; else above if it fits; else pick
  //      whichever side is larger and shrink the popup to fit
  //      with an internal scroll, so the caret side is always clear.
  //   3. Clamp the horizontal position to the viewport without
  //      violating the vertical clearance.
  const caret = caretRect() || editor.getBoundingClientRect();
  const caretBottom = caret.bottom || caret.top + 18;
  const caretTop = caret.top || (caretBottom - 18);
  const caretHeight = Math.max(16, caretBottom - caretTop);

  // Reset any max-height left over from a previous shrink so the
  // popup reports its natural size on the first measurement.
  suggestionsEl.style.maxHeight = '';
  suggestionsEl.style.overflowY = '';

  const box = suggestionsEl.getBoundingClientRect();
  const popupW = Math.max(260, box.width || 260);
  let popupH = Math.max(60, box.height || 60);

  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const gap = 10 + Math.round(caretHeight * 0.2);
  const viewportMargin = 8;

  // Effective viewport bounds (excluding chrome that floats over content).
  let effTop = 0;
  let effBottom = vh;
  const tabBar = document.getElementById('tab-bar');
  if (tabBar) {
    const r = tabBar.getBoundingClientRect();
    if (r.bottom > effTop && r.bottom < vh) effTop = r.bottom;
  }
  const styleBar = document.getElementById('style-bar');
  if (styleBar && !styleBar.hidden) {
    const r = styleBar.getBoundingClientRect();
    if (r.top > effTop && r.top < effBottom) effBottom = r.top;
  }
  const statusBar = document.getElementById('status-bar');
  if (statusBar) {
    const r = statusBar.getBoundingClientRect();
    if (r.top > effTop && r.top < effBottom) effBottom = r.top;
  }
  effTop += viewportMargin;
  effBottom -= viewportMargin;

  const spaceBelow = effBottom - caretBottom - gap;
  const spaceAbove = caretTop - effTop - gap;

  let top;
  if (popupH <= spaceBelow) {
    // Fits below — preferred placement.
    top = caretBottom + gap;
  } else if (popupH <= spaceAbove) {
    // Doesn't fit below but fits above — flip.
    top = caretTop - popupH - gap;
  } else {
    // Neither side fully fits — take the larger side and shrink
    // the popup to fit with an internal scroll, keeping the caret
    // strictly free.
    if (spaceBelow >= spaceAbove) {
      popupH = Math.max(60, spaceBelow);
      top = caretBottom + gap;
    } else {
      popupH = Math.max(60, spaceAbove);
      top = caretTop - popupH - gap;
    }
    suggestionsEl.style.maxHeight = `${popupH}px`;
    suggestionsEl.style.overflowY = 'auto';
  }

  // Final vertical safety: under no circumstance should the popup's
  // rect overlap the caret's rect. If it would, clamp away from it.
  if (top < caretBottom + gap && top + popupH > caretTop - gap) {
    if (top < caretTop) top = caretTop - popupH - gap;
    else top = caretBottom + gap;
  }

  // Horizontal — keep in viewport, don't fight the vertical decision.
  let left = caret.left;
  left = Math.max(viewportMargin, Math.min(left, vw - popupW - viewportMargin));

  suggestionsEl.style.top = `${Math.max(effTop, top)}px`;
  suggestionsEl.style.left = `${left}px`;
}

function hideSuggestions() {
  suggestionsEl.classList.add('hidden');
}

function suggestionsOpen() {
  return !suggestionsEl.classList.contains('hidden') && suggestionItems.length > 0;
}

function moveSuggestion(delta) {
  if (!suggestionsOpen()) return;
  const n = suggestionItems.length;
  suggestionIndex = (suggestionIndex + delta + n) % n;
  renderSuggestions();
}

async function acceptSuggestion() {
  if (!suggestionsOpen()) return;
  const item = suggestionItems[suggestionIndex];
  if (!item) return;
  cancelSuggestionDebounce();
  const resp = await invoke('apply_abbreviation', { id: item.abbr_id });
  suggestionsManuallyDismissed = false;
  await applyResponse(resp);
  hideSuggestions();
  editor.focus();
}

function dismissSuggestions() {
  suggestionsManuallyDismissed = true;
  hideSuggestions();
}

// Backspace 로 cho_seq 매칭을 취소: 후보들의 최대 rollback_chars 만큼
// preedit + committed 양쪽에서 입력 초성을 한꺼번에 제거한다. 백엔드의
// abbr 엔진 commit_tail 도 함께 비워서 다음 입력에 잔재가 남지 않게 한다.
async function cancelAbbrMatch() {
  if (!suggestionsOpen()) return;
  // 후보별 rollback 길이는 보통 같지만 substring 매치가 섞일 수 있으므로
  // 최댓값을 선택해서 사용자가 입력한 cho 시퀀스 전체를 보장 제거한다.
  const total = suggestionItems.reduce(
    (m, it) => Math.max(m, it.rollback_chars || 0), 0,
  );
  const preLen = preedit ? Array.from(preedit).length : 0;
  const fromCommitted = Math.max(0, total - preLen);
  cancelSuggestionDebounce();
  try {
    const resp = await invoke('cancel_abbr_match', { rollbackChars: fromCommitted });
    suggestionsManuallyDismissed = false;
    await applyResponse(resp);
  } catch (_) {
    // 폴백: 백엔드 호출 실패 시 적어도 팝업과 preedit 만은 정리.
    preedit = '';
  }
  hideSuggestions();
  // applyResponse → render() 가 캐럿/preedit 을 갱신하지만 마지막으로
  // 에디터 포커스를 보장.
  editor.focus();
}

// Reset the "manually dismissed" latch whenever typing changes meaningfully.
function rearmSuggestions() {
  suggestionsManuallyDismissed = false;
}

function logEvent(text) {
  if (lastEventEl) lastEventEl.textContent = text;
}

// ── Jamo-granularity backspace on committed text ────────────────────
// Mirrors the FSM's composing-state backspace but operates on the
// already-committed `committed` string. Returns a new string with one
// jamo removed (decomposing compound finals / vowels step-by-step).
function jamoBackspaceCommitted(text) {
  if (!text) return text;
  const cps = Array.from(text);
  const last = cps[cps.length - 1];
  const cp = last.codePointAt(0);

  // NFC Hangul syllable (U+AC00..=U+D7A3).
  if (cp >= 0xAC00 && cp <= 0xD7A3) {
    const idx = cp - 0xAC00;
    const choIdx  = Math.floor(idx / 588);
    const jungIdx = Math.floor((idx % 588) / 28);
    const jongIdx = idx % 28;
    if (jongIdx > 0) {
      const jongCp = 0x11A7 + jongIdx; // 0x11A8 − 1
      const simpler = decomposeJongCp(jongCp);
      const newJongIdx = simpler == null ? 0 : simpler - 0x11A7;
      const newSyl = 0xAC00 + choIdx * 588 + jungIdx * 28 + newJongIdx;
      cps[cps.length - 1] = String.fromCodePoint(newSyl);
      return cps.join('');
    }
    // No jong — try to decompose the vowel next.
    const jungCp = 0x1161 + jungIdx;
    const simplerV = decomposeJungCp(jungCp);
    if (simplerV != null) {
      const newJungIdx = simplerV - 0x1161;
      const newSyl = 0xAC00 + choIdx * 588 + newJungIdx * 28;
      cps[cps.length - 1] = String.fromCodePoint(newSyl);
      return cps.join('');
    }
    // No vowel decomposition — leaves a lone Cho conjoining jamo.
    const choCp = 0x1100 + choIdx;
    cps[cps.length - 1] = String.fromCodePoint(choCp);
    return cps.join('');
  }

  // Compound conjoining Jong (U+11A8..=U+11C2) — decompose one step.
  if (cp >= 0x11A8 && cp <= 0x11C2) {
    const simpler = decomposeJongCp(cp);
    if (simpler != null) {
      cps[cps.length - 1] = String.fromCodePoint(simpler);
      return cps.join('');
    }
  }
  // Compound conjoining Jung (U+1161..=U+1175).
  if (cp >= 0x1161 && cp <= 0x1175) {
    const simpler = decomposeJungCp(cp);
    if (simpler != null) {
      cps[cps.length - 1] = String.fromCodePoint(simpler);
      return cps.join('');
    }
  }

  // Non-Hangul or simple jamo — drop the code point outright.
  cps.pop();
  return cps.join('');
}

function decomposeJongCp(cp) {
  switch (cp) {
    case 0x11A9: case 0x11AA: return 0x11A8; // ᆩ/ᆪ → ᆨ
    case 0x11AC: case 0x11AD: return 0x11AB; // ᆬ/ᆭ → ᆫ
    case 0x11B0: case 0x11B1: case 0x11B2: case 0x11B3:
    case 0x11B4: case 0x11B5: case 0x11B6: return 0x11AF; // ᆰ~ᆶ → ᆯ
    case 0x11B9: return 0x11B8; // ᆹ → ᆸ
    case 0x11BB: return 0x11BA; // ᆻ → ᆺ
    default: return null;
  }
}

function decomposeJungCp(cp) {
  switch (cp) {
    case 0x116A: case 0x116B: case 0x116C: return 0x1169; // ᅪ/ᅫ/ᅬ → ᅩ
    case 0x116F: case 0x1170: case 0x1171: return 0x116E; // ᅯ/ᅰ/ᅱ → ᅮ
    case 0x1174: return 0x1173; // ᅴ → ᅳ
    default: return null;
  }
}

let lastAppliedMode = 'hangul';
function applyInputMode(mode) {
  if (mode === lastAppliedMode) return;
  lastAppliedMode = mode;
  if (mode === 'english') {
    langCurrentEl.textContent = 'EN';
    langOtherEl.textContent = '한';
  } else {
    langCurrentEl.textContent = '한';
    langOtherEl.textContent = 'EN';
  }
  // Keyboard map reflects the active (mode-dependent) layout.
  renderKeyboardMap();
}

editor.addEventListener('keydown', async (ev) => {
  // If the help modal is open, let the global handler deal with it.
  if (isHelpOpen()) return;

  // Modifier-only keydowns (Shift/Control/Alt/Meta without a companion
  // letter) must not reach the backend — they arrive as 'ShiftLeft' etc.
  // which parse_keycode rejects and would otherwise flush the preedit,
  // orphaning the jong that the *next* Shift+letter keydown produces.
  if (ev.key === 'Shift' || ev.key === 'Control'
      || ev.key === 'Alt' || ev.key === 'Meta') {
    return;
  }

  // OS IME bypass: let the system input method and native contenteditable
  // handle everything (composition, clipboard, undo/redo, shortcuts).
  if (isOsImeMode()) {
    return;
  }

  // ── Suggestion picker: intercept navigation/accept keys ───────────
  if (suggestionsOpen()) {
    if (ev.code === 'Tab' || ev.key === 'Enter' && ev.shiftKey === false && ev.metaKey === false) {
      // Tab always accepts. Enter accepts *only* when not inserting a
      // newline is the user's intent — we fall through to Enter handling
      // otherwise. For simplicity: Tab accepts, Enter passes through.
      if (ev.code === 'Tab') {
        ev.preventDefault();
        await acceptSuggestion();
        return;
      }
    }
    if (ev.code === 'ArrowDown') {
      ev.preventDefault();
      moveSuggestion(1);
      return;
    }
    if (ev.code === 'ArrowUp') {
      ev.preventDefault();
      moveSuggestion(-1);
      return;
    }
    // Horizontal arrows: dismiss the popup and let the caret move natively.
    if (ev.code === 'ArrowLeft' || ev.code === 'ArrowRight') {
      dismissSuggestions();
      // Fall through so the arrow reaches the editor for caret movement;
      // the rearm below is gated to skip arrow/delete keys so the popup
      // stays closed until the user types a fresh content key.
    }
    if (ev.code === 'Escape') {
      ev.preventDefault();
      dismissSuggestions();
      return;
    }
    // Backspace: 입력한 초성 시퀀스 전체를 한 번에 지우고 팝업을 닫는다.
    // 자모 단위로 한 글자씩 지우는 기본 Backspace 보다 cho_seq 매칭을
    // "취소" 하는 의도에 더 가깝다.
    if (ev.code === 'Backspace' && !ev.metaKey && !ev.ctrlKey && !ev.altKey) {
      ev.preventDefault();
      await cancelAbbrMatch();
      return;
    }
  }

  // Rearm the popup only for keys that ADD content. Deletion and caret
  // navigation should leave `suggestionsManuallyDismissed` intact so
  // stale matches don't pop up mid-delete or while navigating.
  const isDeleteKey = ev.code === 'Backspace' || ev.code === 'Delete';
  const isArrowKey = ev.code === 'ArrowLeft' || ev.code === 'ArrowRight'
                  || ev.code === 'ArrowUp' || ev.code === 'ArrowDown'
                  || ev.code === 'Home' || ev.code === 'End'
                  || ev.code === 'PageUp' || ev.code === 'PageDown';
  if (!isDeleteKey && !isArrowKey
      && ev.key !== 'Shift' && ev.key !== 'Control'
      && ev.key !== 'Alt' && ev.key !== 'Meta') {
    rearmSuggestions();
  }
  if (isDeleteKey) dismissSuggestions();

  // ── Block image caret-stop ───────────────────────────────────────
  // If Up/Down would move past a rendered image, intercept: first
  // press selects the image, a second press moves the caret past it.
  if (markdownMode && !preedit && !ev.shiftKey && !ev.metaKey && !ev.ctrlKey && !ev.altKey) {
    if (ev.code === 'ArrowUp' && tryImageCaretStopUp()) {
      ev.preventDefault();
      return;
    }
    if (ev.code === 'ArrowDown' && tryImageCaretStopDown()) {
      ev.preventDefault();
      return;
    }
    // ── 표 셀 간 위/아래 방향키 네비게이션 ─────────────────────────
    // td.md-cell 안에서 Up/Down 시 같은 column 의 인접 행으로 caret 이동.
    // 브라우저 native 표 모델이 .md-syn(contentEditable=false) 파이프 스팬과
    // .md-table-sep(display:none) 의 조합으로 종종 vertical navigation 을
    // 흘려 보내는 케이스가 있어 명시 처리한다.
    if (ev.code === 'ArrowUp' && tryTableArrowNav('up')) {
      ev.preventDefault();
      return;
    }
    if (ev.code === 'ArrowDown' && tryTableArrowNav('down')) {
      ev.preventDefault();
      return;
    }
  }
  // Any other key ends image selection so the user's next action
  // (typing, horizontal arrows, etc.) resumes normal caret behavior.
  if (selectedImage) deselectImage();

  // ── Caret/selection navigation: let the browser handle it natively
  // so Shift+Arrow extends the selection, plain Arrow moves the caret,
  // Home/End jump to line boundaries, etc. If we fall through to the
  // IME forwarder below, `ev.preventDefault()` would block selection.
  if (isArrowKey) {
    // Flush any pending Hangul composition so the caret that the
    // browser is about to move isn't stuck inside a now-outdated
    // preedit span. Fire-and-forget — the flush finishes quickly and
    // the new cursor position is re-synced via the keyup handler.
    if (preedit) {
      invoke('flush').catch(() => null).then((resp) => {
        if (resp && resp.commit) {
          committed = committed.slice(0, cursor) + resp.commit + committed.slice(cursor);
          cursor += resp.commit.length;
        }
        preedit = '';
      });
    }
    return;
  }

  // ── Selection + Backspace/Delete = delete the selection ───────────
  if ((ev.code === 'Backspace' || ev.code === 'Delete') && !ev.metaKey && !ev.ctrlKey) {
    const sel = window.getSelection();
    if (sel && !sel.isCollapsed) {
      ev.preventDefault();
      snapshot();
      const range = selectionInCommitted();
      if (range && range[0] !== range[1]) {
        committed = committed.slice(0, range[0]) + committed.slice(range[1]);
        cursor = range[0];
      }
      // Always drop the preedit on selection-delete — the selection
      // may have included preedit text.
      if (preedit) {
        preedit = '';
        await invoke('cancel_composition').catch(() => null);
      }
      render();
      updateSuggestions([]);
      logEvent('delete selection');
      return;
    }
  }

  // ── Markdown shortcuts ───────────────────────────────────────────
  if (markdownMode && (ev.metaKey || ev.ctrlKey)) {
    const handled = await handleMdShortcut(ev);
    if (handled) { ev.preventDefault(); return; }
  }

  // ── Markdown list auto-continue on Enter ─────────────────────────
  if (markdownMode && ev.code === 'Enter' && !ev.shiftKey && !ev.metaKey && !ev.ctrlKey) {
    if (await mdListContinue()) { ev.preventDefault(); return; }
  }

  // ── Tab / Shift-Tab: indent/outdent current list item ────────────
  if (markdownMode && ev.code === 'Tab' && !ev.metaKey && !ev.ctrlKey) {
    if (await mdIndent(ev.shiftKey ? -1 : 1)) { ev.preventDefault(); return; }
  }

  // ── Clipboard / undo shortcuts (Cmd on macOS, Ctrl elsewhere) ─────
  // Alt 가 함께 눌린 chord (⌥⌘C, ⌥⌘X 등) 는 본문 메뉴 단축키이므로
  // 이 블록이 먹어 버리면 의도한 동작이 안 난다. handleMdShortcut 이
  // 이미 처리했으면 위에서 return 됐을 것이고, 여기까지 왔다면 clipboard
  // 계열이 아닌 Alt+Cmd chord 이므로 조용히 넘긴다.
  const mod = (ev.metaKey || ev.ctrlKey) && !ev.altKey;
  if (mod) {
    if (ev.code === 'KeyZ' && !ev.shiftKey) {
      ev.preventDefault();
      await undo();
      logEvent('undo (↩︎ 뒤로가기)');
      return;
    }
    // Redo: Cmd+Shift+Z (macOS convention), Ctrl+Y (Windows), and
    // Ctrl+F (user-specified "앞으로가기").
    if (
      (ev.code === 'KeyZ' && ev.shiftKey)
      || ev.code === 'KeyY'
      || ev.code === 'KeyF'
    ) {
      ev.preventDefault();
      await redo();
      logEvent('redo (↪︎ 앞으로가기)');
      return;
    }
    if (ev.code === 'KeyX') {
      ev.preventDefault();
      await doCut();
      return;
    }
    if (ev.code === 'KeyV') {
      ev.preventDefault();
      await doPaste();
      return;
    }
    if (ev.code === 'KeyC') {
      ev.preventDefault();
      await doCopy();
      return;
    }
    // Cmd+A: if the caret is inside a fenced code block, narrow the
    // select-all to that block's code lines only. Otherwise fall through
    // to the browser's native select-all over the whole document.
    if (ev.code === 'KeyA') {
      if (selectAllInCodeBlock()) { ev.preventDefault(); return; }
      return;
    }
  }

  // Intercept Caps Lock as a language toggle key; also prevent the
  // browser's default side-effects (e.g. caret-reset) while letting
  // the backend see it.
  if (ev.code === 'CapsLock') {
    ev.preventDefault();
  }

  // Let browser natives handle other modifier chords.
  if (ev.metaKey && ev.key !== 'Shift') return;
  if (ev.ctrlKey && ev.code !== 'Backspace') return;

  // Intercept everything else and send to the IME core.
  ev.preventDefault();

  // ── 선택 영역 대체 ────────────────────────────────────────────────
  // 일반 편집기 동작: 문자가 선택된 상태에서 글자를 치면 선택 범위가
  // 지워지고 그 위치에 새 글자가 입력되어야 한다. 이 에디터는 `committed`
  // 와 custom cursor 를 진실로 삼기 때문에 브라우저의 기본 replace 동작에
  // 기댈 수 없어, IME 로 넘기기 전에 우리가 직접 선택 범위를 잘라낸다.
  // 단, Backspace/Delete/화살표 등은 위에서 이미 return 해 여기까지 오지
  // 않고, 모디파이어 단독 (Shift/Ctrl/Meta/Alt) 도 위 가드로 걸러졌다.
  // 여기 도달하는 키는 대부분 "문자를 만들 수 있는" 키다. 그 중 실제로
  // 문자를 생성할 key 만(단일 문자 키 + Space/Enter) 선택을 대체 처리한다.
  {
    const isProductive =
      (ev.key && ev.key.length === 1) ||
      ev.code === 'Space' || ev.code === 'Enter';
    if (isProductive) {
      const sel = window.getSelection();
      if (sel && !sel.isCollapsed) {
        snapshot();
        const range = selectionInCommitted();
        if (range && range[0] !== range[1]) {
          committed = committed.slice(0, range[0]) + committed.slice(range[1]);
          cursor = range[0];
        }
        if (preedit) {
          preedit = '';
          await invoke('cancel_composition').catch(() => null);
        }
        // 에디터 DOM 을 새 cursor 위치로 재렌더해 브라우저 selection 을
        // 지워 둔다. 이어지는 ime_key_input 은 이 cursor 를 기준으로
        // 한 글자만 삽입한다.
        render();
      }
    }
  }

  const resp = await invoke('ime_key_input', {
    ev: {
      code: ev.code,
      shift: ev.shiftKey,
      ctrl: ev.ctrlKey,
      alt: ev.altKey,
      meta: ev.metaKey,
    },
  });

  // Frontend interprets Backspace-while-idle against the committed buffer.
  if (ev.code === 'Backspace' && resp.passthrough && !resp.commit) {
    if (cursor === 0) {
      render();
      return;
    }
    snapshot({ coalesce: true });
    const mode = bsModeSelect?.value || 'syllable';
    const before = committed.slice(0, cursor);
    const after = committed.slice(cursor);
    let newBefore;
    if (mode === 'jamo') {
      newBefore = jamoBackspaceCommitted(before);
    } else {
      const cps = Array.from(before);
      cps.pop();
      newBefore = cps.join('');
    }
    committed = newBefore + after;
    cursor = newBefore.length;
    preedit = resp.preedit || '';
    render();
    logEvent(mode === 'jamo' ? 'backspace (자모)' : 'backspace (음절)');
    return;
  }

  // Tab: swallow (reserved for future layout cycling).
  if (ev.code === 'Tab') {
    return;
  }

  let fallbackChar = null;
  if (ev.code.startsWith('Key') && ev.key.length === 1) {
    fallbackChar = ev.key;
  } else if (ev.code.startsWith('Digit') && ev.key.length === 1) {
    fallbackChar = ev.key;
  } else if (['Minus', 'Equal', 'Semicolon', 'Quote', 'Comma',
              'Period', 'Slash', 'Backslash', 'Backquote',
              'BracketLeft', 'BracketRight'].includes(ev.code) && ev.key.length === 1) {
    fallbackChar = ev.key;
  }

  await applyResponse(resp, { fallbackChar });
});

layoutSelect.addEventListener('change', async () => {
  const wasOsIme = isOsImeMode();
  if (wasOsIme) {
    // Leaving OS IME: pull whatever the system IME typed into our
    // committed buffer so the normal render path stays consistent.
    committed = (editor.innerText || editor.textContent || '').replace(/\r\n/g, '\n');
    preedit = '';
    cursor = committed.length;
  }

  // Flush any in-flight preedit into committed first.
  const flushed = await invoke('flush');
  if (flushed.commit) {
    committed = committed.slice(0, cursor) + flushed.commit + committed.slice(cursor);
    cursor += flushed.commit.length;
  }
  preedit = '';
  const info = await invoke('set_layout', { id: layoutSelect.value });
  currentHangulLayoutId = info.hangul_layout_id;
  fsmStateEl.textContent = 'Empty';
  lastEventEl.textContent = `layout → ${info.name}`;
  // Force a one-shot DOM sync regardless of mode so the editor reflects
  // the current committed buffer on both entry to and exit from OS IME.
  renderCore();
  if (isOsImeMode()) {
    cancelSuggestionDebounce();
    hideSuggestions();
    suggestionItems = [];
  }
  await renderKeyboardMap();
  await updateStatus();
});

formSelect.addEventListener('change', async () => {
  await invoke('set_output_form', { form: formSelect.value });
  lastEventEl.textContent = `output form → ${formSelect.value}`;
});

bsModeSelect.addEventListener('change', async () => {
  await invoke('set_backspace_mode', { mode: bsModeSelect.value });
  logEvent(`지움 단위 → ${bsModeSelect.value === 'jamo' ? '자모' : '음절'}`);
});

moachigiToggle.addEventListener('change', async () => {
  const mode = moachigiToggle.checked ? 'moachigi' : 'sequential';
  // Flush any in-flight preedit before switching.
  const flushed = await invoke('flush');
  if (flushed.commit) {
    committed = committed.slice(0, cursor) + flushed.commit + committed.slice(cursor);
    cursor += flushed.commit.length;
  }
  preedit = '';
  await invoke('set_compose_mode', { mode });
  fsmStateEl.textContent = 'Empty';
  lastEventEl.textContent = `compose mode → ${mode}`;
  render();
});

/* ─────────── 오탈자 교정 (완성음절 앞 홀자모 정리) ─────────── */
// 예) "ㅎ" 뒤에 바로 "하" 를 치면 앞의 ㅎ는 오타로 보고 지운다. 마찬가지로
// "ㅏ하" → "하", "ᆷ하" → "하". 음절이 완성되기 전의 낱자모(compat 또는
// conjoining) 가 바로 앞에 있을 때만 동작한다. localStorage 기본값 = on.
const TYPO_CORRECT_KEY = 'leaf-ime:typo-correct';
let typoCorrectEnabled = (() => {
  try {
    const v = localStorage.getItem(TYPO_CORRECT_KEY);
    return v === null ? true : v === '1';
  } catch { return true; }
})();
function isCompleteHangulSyllable(ch) {
  if (!ch) return false;
  const c = ch.charCodeAt(0);
  return c >= 0xAC00 && c <= 0xD7A3;
}
function isOrphanJamo(ch) {
  if (!ch) return false;
  const c = ch.charCodeAt(0);
  // 호환 자모(ㄱ-ㅎ, ㅏ-ㅣ): U+3131-U+318E
  if (c >= 0x3131 && c <= 0x318E) return true;
  // 조합용 자모(ᄀ-ᇿ): U+1100-U+11FF
  if (c >= 0x1100 && c <= 0x11FF) return true;
  return false;
}
// 커밋 직후 cursor 위치에서 두 가지 규칙을 차례로 점검하고, 일치하면
// 해당 문자를 지운다. 반환값은 제거한 문자 수.
//
//   1) 줄바꿈(`\n`) 직전 **공백·탭** — 모두 제거
//      예: "검토 부탁드립니다. " + Enter → "검토 부탁드립니다.\n"
//   2) 완성 음절(가-힣) 직전 **홀자모(초·중·종)** — 한 글자 제거
//      예: "ㅎ하" → "하", "ᆷ하" → "하"
// 마지막 교정 결과를 호출자에게 알릴 임시 보관소. 같은 applyResponse
// 사이클 안에서 render() 직후 시각적 피드백을 띄우는 용도.
let lastTypoCorrection = null;

function typoCorrectAfterCommit() {
  lastTypoCorrection = null;
  if (!typoCorrectEnabled) return 0;
  if (cursor < 1) return 0;
  const last = committed.charAt(cursor - 1);
  // 규칙 1: 줄바꿈 직전의 trailing 공백/탭을 일괄 제거.
  if (last === '\n') {
    let count = 0;
    while (cursor - 2 - count >= 0) {
      const ch = committed.charAt(cursor - 2 - count);
      if (ch !== ' ' && ch !== '\t') break;
      count++;
    }
    if (count > 0) {
      committed = committed.slice(0, cursor - 1 - count) + committed.slice(cursor - 1);
      lastTypoCorrection = { kind: 'whitespace', removed: count };
      return count;
    }
    return 0;
  }
  // 규칙 2: 완성 음절 + 직전 홀자모 → 홀자모 한 글자 제거.
  if (cursor < 2) return 0;
  const prev = committed.charAt(cursor - 2);
  if (!isCompleteHangulSyllable(last)) return 0;
  if (!isOrphanJamo(prev)) return 0;
  committed = committed.slice(0, cursor - 2) + committed.slice(cursor - 1);
  lastTypoCorrection = { kind: 'orphan-jamo', removed: 1 };
  return 1;
}

// 오탈자 교정이 일어났을 때 캐럿 부근에서 미세한 펄스 애니메이션을
// 띄워 "방금 무언가가 교정됐다" 라는 피드백을 준다. 한 번만 노출되고
// 자동으로 사라진다 — 중복 누적되지 않게 이전 노드는 즉시 제거한다.
function flashTypoCorrection(kind) {
  let rect;
  try {
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0) return;
    const r = sel.getRangeAt(0);
    rect = r.getBoundingClientRect();
    // 폭이 0인 caret-only range 면 클라이언트 좌표만 사용.
    if (!rect || (rect.width === 0 && rect.height === 0
                  && rect.left === 0 && rect.top === 0)) {
      return;
    }
  } catch { return; }
  // 직전 피드백이 아직 살아 있으면 정리.
  document.querySelectorAll('.typo-correction-flash').forEach((n) => n.remove());
  const flash = document.createElement('div');
  flash.className = 'typo-correction-flash';
  flash.dataset.kind = kind || 'orphan-jamo';
  flash.style.left = `${rect.left}px`;
  flash.style.top = `${rect.top + rect.height / 2}px`;
  document.body.appendChild(flash);
  flash.addEventListener('animationend', () => flash.remove(), { once: true });
  // 안전망: 애니메이션 이벤트가 누락되어도 1초 안에 청소.
  setTimeout(() => { if (flash.isConnected) flash.remove(); }, 1000);
}
const typoCorrectToggle = document.getElementById('typo-correct-toggle');
if (typoCorrectToggle) {
  typoCorrectToggle.checked = typoCorrectEnabled;
  typoCorrectToggle.addEventListener('change', () => {
    typoCorrectEnabled = !!typoCorrectToggle.checked;
    try { localStorage.setItem(TYPO_CORRECT_KEY, typoCorrectEnabled ? '1' : '0'); } catch {}
    logEvent(`오탈자 교정 ${typoCorrectEnabled ? '켬' : '끔'}`);
  });
}

abbrToggle.addEventListener('change', async () => {
  const enabled = abbrToggle.checked;
  await invoke('set_suggestions_enabled', { enabled });
  if (!enabled) {
    hideSuggestions();
    suggestionsManuallyDismissed = true;
  } else {
    suggestionsManuallyDismissed = false;
  }
  logEvent(`자동완성 ${enabled ? '켬' : '끔'}`);
});

latinSelect.addEventListener('change', async () => {
  const info = await invoke('set_latin_layout', { id: latinSelect.value });
  lastEventEl.textContent = `영문 자판 → ${info.name}`;
  await renderKeyboardMap();
});

langToggle.addEventListener('click', async () => {
  const info = await invoke('current_layout');
  const next = info.input_mode === 'hangul' ? 'english' : 'hangul';
  const flushed = await invoke('flush');
  if (flushed.commit) {
    committed = committed.slice(0, cursor) + flushed.commit + committed.slice(cursor);
    cursor += flushed.commit.length;
  }
  preedit = '';
  const updated = await invoke('set_input_mode', { mode: next });
  fsmStateEl.textContent = 'Empty';
  lastEventEl.textContent = `언어 → ${updated.input_mode === 'english' ? '영문' : '한글'} (${updated.name})`;
  render();
  await renderKeyboardMap();
  await updateStatus();
});

// Flush preedit on blur so it doesn't get orphaned.
editor.addEventListener('blur', async () => {
  if (isOsImeMode()) return;
  const resp = await invoke('flush');
  if (resp.commit) {
    committed = committed.slice(0, cursor) + resp.commit + committed.slice(cursor);
    cursor += resp.commit.length;
    preedit = '';
    render();
  }
});

// Block every browser/OS text-insertion path. The Rust FSM is the sole
// authority over the editor contents; without this, a native macOS
// input method or the contenteditable default handling can race with
// our keydown handler and produce stray characters during composition.
// In OS IME mode these blocks are lifted so the system IME can drive
// the editor natively.
editor.addEventListener('beforeinput', (ev) => {
  if (isOsImeMode()) return;
  ev.preventDefault();
});
editor.addEventListener('compositionstart', (ev) => {
  if (isOsImeMode()) return;
  ev.preventDefault();
});
editor.addEventListener('compositionupdate', (ev) => {
  if (isOsImeMode()) return;
  ev.preventDefault();
});
editor.addEventListener('compositionend', (ev) => {
  if (isOsImeMode()) return;
  ev.preventDefault();
});
// ───────────────────────── Clipboard + Undo/Redo ────────────────────
//
// The IME-controlled model keeps all displayed text in two JS strings
// (`committed` + `preedit`), so the browser's native clipboard and
// history mechanisms don't see the full content. We intercept the
// relevant events and drive the state ourselves.

const UNDO_CAPACITY = 200;
const UNDO_COALESCE_MS = 500;
const undoStack = [];
const redoStack = [];
let lastSnapshotAt = 0;

function snapshot({ coalesce = false } = {}) {
  const now = Date.now();
  const top = undoStack[undoStack.length - 1];
  if (
    coalesce
    && top
    && top.committed === committed
    && top.preedit === preedit
    && top.cursor === cursor
  ) {
    return;
  }
  if (
    coalesce
    && top
    && now - lastSnapshotAt < UNDO_COALESCE_MS
    && undoStack.length > 0
  ) {
    // Update the most recent snapshot in place rather than appending —
    // treats a burst of keystrokes as a single undo step.
    top.committed = committed;
    top.preedit = preedit;
    top.cursor = cursor;
    lastSnapshotAt = now;
    return;
  }
  undoStack.push({ committed, preedit, cursor });
  if (undoStack.length > UNDO_CAPACITY) undoStack.shift();
  redoStack.length = 0;
  lastSnapshotAt = now;
}

async function undo() {
  if (undoStack.length === 0) return;
  const snap = undoStack.pop();
  redoStack.push({ committed, preedit, cursor });
  committed = snap.committed;
  preedit = snap.preedit;
  if (typeof snap.cursor === 'number') cursor = snap.cursor;
  else cursor = committed.length;
  // Cancel any in-progress FSM state on the backend to keep UIs in sync.
  await invoke('flush').catch(() => {});
  suggestionsManuallyDismissed = true;
  render();
  updateSuggestions([]);
}

async function redo() {
  if (redoStack.length === 0) return;
  const snap = redoStack.pop();
  undoStack.push({ committed, preedit, cursor });
  committed = snap.committed;
  preedit = snap.preedit;
  if (typeof snap.cursor === 'number') cursor = snap.cursor;
  else cursor = committed.length;
  await invoke('flush').catch(() => {});
  suggestionsManuallyDismissed = true;
  render();
  updateSuggestions([]);
}

/**
 * Maps a DOM node/offset inside `editor` to a character index in the
 * `committed` string. Returns `null` when the point is inside the
 * preedit span or the caret probe (those aren't part of `committed`).
 */
function committedIndexOf(node, offset) {
  let idx = 0;
  let found = null;
  function isSkipped(n) {
    return n
      && n.classList
      && (n.classList.contains('preedit') || n.classList.contains('caret-probe'));
  }
  function walk(n) {
    if (found !== null) return;
    if (n === node) {
      if (n.nodeType === Node.TEXT_NODE) {
        found = idx + offset;
      } else {
        // element — `offset` is a child index.
        let acc = idx;
        for (let i = 0; i < n.childNodes.length && i < offset; i++) {
          const c = n.childNodes[i];
          if (!isSkipped(c)) acc += measure(c);
        }
        found = acc;
      }
      return;
    }
    if (isSkipped(n)) return;
    if (n.nodeType === Node.TEXT_NODE) {
      idx += n.length;
    } else if (n.nodeName === 'BR') {
      idx += 1;
    } else {
      for (const c of n.childNodes) walk(c);
    }
  }
  function measure(n) {
    if (isSkipped(n)) return 0;
    if (n.nodeType === Node.TEXT_NODE) return n.length;
    if (n.nodeName === 'BR') return 1;
    let t = 0;
    for (const c of n.childNodes) t += measure(c);
    return t;
  }
  walk(editor);
  return found;
}

/**
 * Returns `[start, end]` indices in `committed` for the current
 * selection, or `null` when the selection isn't entirely inside the
 * committed region.
 */
function selectionInCommitted() {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return null;
  const r = sel.getRangeAt(0);
  // Selection must be inside the editor (anchor or common ancestor).
  const inside = (n) => n === editor || editor.contains(n);
  if (!inside(r.startContainer) || !inside(r.endContainer)) return null;
  // Use the markdown-aware mapper so ⌘A across multiple `.md-line`
  // blocks counts the implicit `\n` between them. Plain mode's path
  // inside domToSourceIdx walks text nodes + <br> just like the old
  // committedIndexOf did.
  const start = domToSourceIdx(r.startContainer, r.startOffset);
  const end = domToSourceIdx(r.endContainer, r.endOffset);
  if (typeof start !== 'number' || typeof end !== 'number') return null;
  const clamp = (i) => Math.max(0, Math.min(i, committed.length));
  return [clamp(Math.min(start, end)), clamp(Math.max(start, end))];
}

// Cmd+A 가 코드블록 내부에서 눌렸으면 그 블록의 .md-code-line 들만 선택한다.
// 코드블록 바깥이면 false 를 반환해 네이티브 전체 선택을 타게 둔다.
function selectAllInCodeBlock() {
  if (!markdownMode) return false;
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return false;
  const focusNode = sel.focusNode;
  if (!focusNode || !editor.contains(focusNode)) return false;
  const focusEl = focusNode.nodeType === 1 ? focusNode : focusNode.parentElement;
  const line = focusEl?.closest?.('.md-code-line, .md-fence-open, .md-fence-close');
  if (!line || !editor.contains(line)) return false;

  // 형제를 거슬러 올라가며 감싸는 fence-open / fence-close 를 찾는다.
  // 사이에 낀 요소가 .md-code-line / .md-fence-* 가 아니면 같은 블록이
  // 아니므로 탐색을 중단한다.
  let openFence = line.classList.contains('md-fence-open') ? line : null;
  for (let c = line.previousElementSibling; c && !openFence; c = c.previousElementSibling) {
    if (c.classList.contains('md-fence-open')) { openFence = c; break; }
    if (!c.classList.contains('md-code-line')) return false;
  }
  let closeFence = line.classList.contains('md-fence-close') ? line : null;
  for (let c = line.nextElementSibling; c && !closeFence; c = c.nextElementSibling) {
    if (c.classList.contains('md-fence-close')) { closeFence = c; break; }
    if (!c.classList.contains('md-code-line')) return false;
  }
  if (!openFence || !closeFence) return false;

  const firstCode = openFence.nextElementSibling;
  const lastCode = closeFence.previousElementSibling;
  // 빈 코드블록(``` 바로 다음 줄이 ```)은 네이티브 전체 선택을 타게 둔다.
  if (!firstCode || firstCode === closeFence) return false;
  if (!firstCode.classList.contains('md-code-line')) return false;
  if (!lastCode || !lastCode.classList.contains('md-code-line')) return false;

  const range = document.createRange();
  range.setStart(firstCode, 0);
  range.setEnd(lastCode, lastCode.childNodes.length);
  sel.removeAllRanges();
  sel.addRange(range);
  return true;
}

// ─── Clipboard bridge ─────────────────────────────────────────────
// Use three levels of fallback so copy/paste works on every platform
// the Tauri WebView runs on:
//   1. Tauri `clipboard-manager` plugin — most reliable inside the app.
//   2. `navigator.clipboard` — standard browser API.
//   3. `document.execCommand('copy')` — legacy fallback for write only.
const clipboardApi = window.__TAURI__?.clipboardManager;

async function clipboardWrite(text) {
  if (clipboardApi?.writeText) {
    try { await clipboardApi.writeText(text); return true; } catch {}
  }
  if (navigator.clipboard?.writeText) {
    try { await navigator.clipboard.writeText(text); return true; } catch {}
  }
  // Last-ditch fallback using a hidden textarea + execCommand.
  try {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand('copy');
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}

async function clipboardRead() {
  if (clipboardApi?.readText) {
    try { return await clipboardApi.readText(); } catch {}
  }
  if (navigator.clipboard?.readText) {
    try { return await navigator.clipboard.readText(); } catch {}
  }
  return '';
}

async function doCopy() {
  const sel = window.getSelection();
  const text = sel ? sel.toString().replace(/\u200b/g, '') : '';
  if (!text) {
    logEvent('복사할 선택 영역이 없습니다');
    return false;
  }
  const ok = await clipboardWrite(text);
  logEvent(ok ? `복사됨: "${truncate(text, 30)}"` : '복사 실패');
  return ok;
}

async function doCut() {
  const sel = window.getSelection();
  const text = sel ? sel.toString().replace(/\u200b/g, '') : '';
  if (!text) {
    logEvent('잘라낼 선택 영역이 없습니다');
    return;
  }
  const ok = await clipboardWrite(text);
  if (!ok) {
    logEvent('잘라내기 실패 (클립보드 접근 불가)');
    return;
  }
  const range = selectionInCommitted();
  if (!range || range[0] === range[1]) {
    logEvent(`복사됨: "${truncate(text, 30)}"`);
    return;
  }
  snapshot();
  committed = committed.slice(0, range[0]) + committed.slice(range[1]);
  if (preedit) {
    preedit = '';
    await invoke('cancel_composition').catch(() => null);
  }
  render();
  updateSuggestions([]);
  logEvent(`잘라냄: "${truncate(text, 30)}"`);
}

async function doPaste() {
  const text = await clipboardRead();
  if (!text) {
    logEvent('붙여넣을 내용이 없습니다 (또는 권한 거부)');
    return;
  }
  snapshot();
  // Flush any Hangul composition first so the insertion point is stable.
  const flushed = await invoke('flush').catch(() => null);
  if (flushed && flushed.commit) {
    committed = committed.slice(0, cursor) + flushed.commit + committed.slice(cursor);
    cursor += flushed.commit.length;
  }
  preedit = '';
  const range = selectionInCommitted();
  if (range && range[0] !== range[1]) {
    committed = committed.slice(0, range[0]) + text + committed.slice(range[1]);
    cursor = range[0] + text.length;
  } else if (range) {
    const pos = range[0];
    committed = committed.slice(0, pos) + text + committed.slice(pos);
    cursor = pos + text.length;
  } else {
    committed = committed.slice(0, cursor) + text + committed.slice(cursor);
    cursor += text.length;
  }
  render();
  updateSuggestions([]);
  logEvent(`붙여넣음 (${text.length}자)`);
}

function truncate(s, n) {
  if (!s) return '';
  const cps = Array.from(s.replace(/\n/g, '⏎'));
  return cps.length <= n ? cps.join('') : cps.slice(0, n).join('') + '…';
}

// Filter the zero-width probe out of any native copy the browser
// performs on its own (e.g. from a right-click menu).
editor.addEventListener('copy', (ev) => {
  const sel = window.getSelection();
  if (!sel || !sel.toString()) return;
  const text = sel.toString().replace(/\u200b/g, '');
  ev.clipboardData?.setData('text/plain', text);
  ev.preventDefault();
});

// Drop: accept plain text drops only.
editor.addEventListener('drop', async (ev) => {
  ev.preventDefault();
  const text = ev.dataTransfer?.getData('text/plain') || '';
  if (!text) return;
  snapshot();
  committed += text;
  render();
  updateSuggestions([]);
});

async function renderAbbrList() {
  const list = document.getElementById('abbr-list');
  const count = document.getElementById('abbr-count');
  if (!list) return;
  const abbrs = await invoke('list_abbreviations');
  list.innerHTML = '';
  for (const a of abbrs) {
    const row = document.createElement('div');
    row.className = 'abbr-row';
    const trig = document.createElement('span');
    trig.className = 'abbr-trigger';
    trig.textContent = a.trigger;
    const body = document.createElement('span');
    body.className = 'abbr-body';
    body.textContent = a.body.replace(/\n/g, ' ⏎ ');
    body.title = a.body;
    const kind = document.createElement('span');
    kind.className = `abbr-kind ${a.kind}`;
    kind.textContent = ({
      cho_seq: '초성',
      literal: '단어',
      ending: '어미',
    })[a.kind] || '기타';
    row.appendChild(trig);
    row.appendChild(body);
    row.appendChild(kind);
    list.appendChild(row);
  }
  if (count) count.textContent = `${abbrs.length}개`;
}

// Make the contenteditable focus on load for immediate typing.
window.addEventListener('load', async () => {
  await updateStatus();
  await renderKeyboardMap();
  await renderAbbrList();
  restoreTabsAndFiles();
  editor.focus();
  render();
});

/* ═══════════════════════ Custom prompt modal ═══════════════════════ */
// Tauri's webview blocks window.prompt/alert/confirm. Drop-in replacement
// returning a Promise<string | null> (null on cancel / empty).
function askPrompt({ title, body = '', defaultValue = '', placeholder = '',
                     confirmLabel = '확인', cancelLabel = '취소' } = {}) {
  return new Promise((resolve) => {
    const modal = document.getElementById('prompt-modal');
    if (!modal) { resolve(null); return; }
    const h = document.getElementById('prompt-title');
    const p = document.getElementById('prompt-body');
    const input = document.getElementById('prompt-input');
    const okBtn = document.getElementById('prompt-ok');
    const cancelBtn = document.getElementById('prompt-cancel');
    const backdrop = modal.querySelector('.settings-backdrop');
    if (title) h.textContent = title;
    p.textContent = body || '';
    input.value = defaultValue;
    input.placeholder = placeholder;
    okBtn.textContent = confirmLabel;
    cancelBtn.textContent = cancelLabel;

    let cleaned = false;
    const finish = (val) => {
      if (cleaned) return; cleaned = true;
      modal.classList.add('hidden');
      okBtn.removeEventListener('click', onOk);
      cancelBtn.removeEventListener('click', onCancel);
      backdrop.removeEventListener('click', onCancel);
      window.removeEventListener('keydown', onKey, true);
      resolve(val);
      setTimeout(() => editor.focus(), 0);
    };
    const onOk = () => finish(input.value.trim() || null);
    const onCancel = () => finish(null);
    const onKey = (ev) => {
      if (ev.key === 'Escape') { ev.preventDefault(); ev.stopPropagation(); onCancel(); }
      if (ev.key === 'Enter' && document.activeElement === input) {
        ev.preventDefault(); ev.stopPropagation(); onOk();
      }
    };
    okBtn.addEventListener('click', onOk);
    cancelBtn.addEventListener('click', onCancel);
    backdrop.addEventListener('click', onCancel);
    window.addEventListener('keydown', onKey, true);
    modal.classList.remove('hidden');
    // Select the name without the extension so typing replaces the stem.
    setTimeout(() => {
      input.focus();
      const dot = defaultValue.lastIndexOf('.');
      if (dot > 0) input.setSelectionRange(0, dot);
      else input.select();
    }, 0);
  });
}

/* ═══════════════════════ Custom confirm modal ═══════════════════════ */
// Returns a promise resolving to 'save' | 'discard' | 'cancel'.
function askConfirm({ title, body, saveLabel = '저장', discardLabel = '저장 안 함', cancelLabel = '취소', showSave = true } = {}) {
  return new Promise((resolve) => {
    const modal = document.getElementById('confirm-modal');
    const h = document.getElementById('confirm-title');
    const p = document.getElementById('confirm-body');
    const saveBtn = document.getElementById('confirm-save');
    const discardBtn = document.getElementById('confirm-discard');
    const cancelBtn = document.getElementById('confirm-cancel');
    const backdrop = modal.querySelector('.settings-backdrop');
    if (title) h.textContent = title;
    if (body) p.textContent = body;
    saveBtn.textContent = saveLabel;
    discardBtn.textContent = discardLabel;
    cancelBtn.textContent = cancelLabel;
    // 라벨이 빈 문자열이면 버튼 자체를 숨긴다. 그렇지 않으면 빈 박스만
    // 동그랗게 남아 사용자에게 클릭할 수 없는 버튼처럼 보인다.
    const showSaveBtn = !!(showSave && saveLabel);
    const showDiscardBtn = !!discardLabel;
    const showCancelBtn = !!cancelLabel;
    saveBtn.style.display = showSaveBtn ? '' : 'none';
    discardBtn.style.display = showDiscardBtn ? '' : 'none';
    cancelBtn.style.display = showCancelBtn ? '' : 'none';

    let cleaned = false;
    const finish = (result) => {
      if (cleaned) return; cleaned = true;
      modal.classList.add('hidden');
      saveBtn.removeEventListener('click', onSave);
      discardBtn.removeEventListener('click', onDiscard);
      cancelBtn.removeEventListener('click', onCancel);
      backdrop.removeEventListener('click', onCancel);
      window.removeEventListener('keydown', onKey, true);
      resolve(result);
      editor.focus();
    };
    const onSave = () => finish('save');
    const onDiscard = () => finish('discard');
    const onCancel = () => finish('cancel');
    // 키 동작도 보이는 버튼만을 대상으로 한다 — Esc 는 가능한 한 cancel,
    // 없으면 discard 로 떨어지고, Enter 는 우선순위대로 save → discard
    // → cancel 중 첫 번째 활성 버튼을 누른 효과를 낸다.
    const escResult = showCancelBtn ? 'cancel' : showDiscardBtn ? 'discard' : 'save';
    const enterResult = showSaveBtn ? 'save' : showDiscardBtn ? 'discard' : 'cancel';
    const onKey = (ev) => {
      if (ev.key === 'Escape') { ev.preventDefault(); ev.stopPropagation(); finish(escResult); }
      if (ev.key === 'Enter') { ev.preventDefault(); ev.stopPropagation(); finish(enterResult); }
    };
    saveBtn.addEventListener('click', onSave);
    discardBtn.addEventListener('click', onDiscard);
    cancelBtn.addEventListener('click', onCancel);
    backdrop.addEventListener('click', onCancel);
    window.addEventListener('keydown', onKey, true);
    modal.classList.remove('hidden');
    (showSaveBtn ? saveBtn : showDiscardBtn ? discardBtn : cancelBtn).focus();
  });
}

/* ═══════════════════════ Tab state + management ═══════════════════════ */
const TABS_KEY = 'leaf-ime:tabs';
const FILES_DIR_KEY = 'leaf-ime:files-dir';
// In-memory tabs. Each: { id, title, path (optional), committed, savedCommitted, cursor, dirty }.
let tabs = [];
let activeTabId = null;
let nextTabId = 1;

function activeTab() { return tabs.find((t) => t.id === activeTabId) || null; }

function commitTabState() {
  const t = activeTab();
  if (!t) return;
  t.committed = committed;
  t.cursor = cursor;
  t.dirty = t.committed !== t.savedCommitted;
}

function loadTabState(id) {
  const t = tabs.find((x) => x.id === id);
  if (!t) return;
  activeTabId = id;
  committed = t.committed || '';
  cursor = Math.min(t.cursor || 0, committed.length);
  preedit = '';
  renderTabs();
  render();
}

function saveTabs() {
  try {
    const data = {
      tabs: tabs.map((t) => ({
        id: t.id,
        title: t.title,
        path: t.path || null,
        committed: t.committed,
        savedCommitted: t.savedCommitted,
        cursor: t.cursor,
      })),
      nextTabId,
      activeTabId,
    };
    localStorage.setItem(TABS_KEY, JSON.stringify(data));
  } catch (_) {}
}

let saveTabsTimer = null;
function saveTabsDebounced() {
  if (saveTabsTimer) clearTimeout(saveTabsTimer);
  saveTabsTimer = setTimeout(() => { saveTabsTimer = null; saveTabs(); }, 300);
}

function renderTabs() {
  const list = document.getElementById('tab-list');
  if (!list) return;
  list.innerHTML = '';
  for (const t of tabs) {
    const li = document.createElement('li');
    li.className = 'tab' + (t.id === activeTabId ? ' active' : '') + (t.dirty ? ' dirty' : '');
    li.dataset.tabId = String(t.id);
    li.role = 'tab';

    const dot = document.createElement('span');
    dot.className = 'tab-dot';
    li.appendChild(dot);

    const title = document.createElement('span');
    title.className = 'tab-title';
    title.textContent = t.title;
    title.title = t.path || t.title;
    li.appendChild(title);

    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'tab-close';
    close.setAttribute('aria-label', '닫기');
    close.addEventListener('click', async (ev) => {
      ev.stopPropagation();
      await closeTab(t.id);
    });
    li.appendChild(close);

    li.addEventListener('click', () => {
      if (t.id !== activeTabId) {
        commitTabState();
        loadTabState(t.id);
      }
    });
    li.addEventListener('contextmenu', (ev) => openTabCtxMenu(ev, t));
    list.appendChild(li);
  }
}

/* ─────────── Tab right-click menu ─────────── */
function openTabCtxMenu(ev, tab) {
  ev.preventDefault();
  ev.stopPropagation();
  const idx = tabs.findIndex((x) => x.id === tab.id);
  const hasLeft = idx > 0;
  const hasRight = idx < tabs.length - 1;
  const hasOthers = tabs.length > 1;

  const actions = [];
  if (tab.path) {
    actions.push({ label: '이 파일 위치로', act: 'reveal-in-tree' });
    actions.push({ sep: true });
  }
  actions.push({ label: '이 탭 닫기', act: 'close' });
  if (hasLeft) actions.push({ label: '왼쪽 탭 모두 닫기', act: 'close-left' });
  if (hasRight) actions.push({ label: '오른쪽 탭 모두 닫기', act: 'close-right' });
  if (hasOthers) actions.push({ label: '다른 탭 모두 닫기', act: 'close-others' });
  if (tabs.length > 0) {
    actions.push({ sep: true });
    actions.push({ label: '모든 탭 닫기', act: 'close-all' });
  }

  fileCtxMenu.innerHTML = '';
  for (const it of actions) {
    if (it.sep) {
      const s = document.createElement('div');
      s.className = 'ctx-sep';
      fileCtxMenu.appendChild(s);
      continue;
    }
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = it.label;
    b.addEventListener('click', async (e) => {
      e.stopPropagation();
      hideFileCtxMenu();
      await runTabAction(it.act, tab);
    });
    fileCtxMenu.appendChild(b);
  }
  fileCtxMenu.classList.remove('hidden');
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const r = fileCtxMenu.getBoundingClientRect();
  const x = Math.min(ev.clientX, vw - r.width - 8);
  const y = Math.min(ev.clientY, vh - r.height - 8);
  fileCtxMenu.style.left = `${Math.max(8, x)}px`;
  fileCtxMenu.style.top = `${Math.max(8, y)}px`;
}

async function runTabAction(act, tab) {
  if (act === 'reveal-in-tree') {
    await revealTabInTree(tab);
    return;
  }
  // Snapshot ids so array mutations during closeTab don't shift indices.
  const idx = tabs.findIndex((x) => x.id === tab.id);
  let ids = [];
  switch (act) {
    case 'close': ids = [tab.id]; break;
    case 'close-left': ids = tabs.slice(0, idx).map((x) => x.id); break;
    case 'close-right': ids = tabs.slice(idx + 1).map((x) => x.id); break;
    case 'close-others': ids = tabs.filter((x) => x.id !== tab.id).map((x) => x.id); break;
    case 'close-all': ids = tabs.map((x) => x.id); break;
  }
  for (const id of ids) {
    // closeTab is async; await sequentially so dirty-prompts queue nicely.
    await closeTab(id);
  }
}

async function revealTabInTree(tab) {
  if (!tab || !tab.path) return;
  const parent = pathParent(tab.path);
  if (!parent) return;
  rootDir = parent;
  expandedDirs.clear();
  dirCache.clear();
  try { localStorage.setItem(FILES_DIR_KEY, rootDir); } catch (_) {}
  // Un-collapse the files pane so the user can see the result.
  const pane = document.getElementById('files-pane');
  if (pane && pane.classList.contains('collapsed')) {
    pane.classList.remove('collapsed');
    try { localStorage.setItem(FILES_COLLAPSED_KEY, '0'); } catch (_) {}
    reCenterStyleBar();
  }
  await refreshFileList();
  // Highlight + scroll the file into view.
  const li = filesListEl && filesListEl.querySelector(`.file-item[data-path="${CSS.escape(tab.path)}"]`);
  if (li) {
    selectFileItem(li);
    li.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }
}

function updateStatusBar() {
  const el = document.getElementById('status-path');
  if (!el) return;
  const t = activeTab();
  if (!t) { el.textContent = ''; el.title = ''; updateWindowTitle(); return; }
  // 경로가 있으면 hover tooltip 으로 노출하고, 없으면 비워 둔다.
  // "편집됨 / 저장 안 됨" 상태는 창 제목과 탭 dot 으로 이미 드러나므로
  // 상태바에는 별도 문구를 띄우지 않는다.
  el.textContent = '';
  el.title = t.path || '';
  updateWindowTitle();
}

let lastWindowTitle = '';
function updateWindowTitle() {
  const t = activeTab();
  let title;
  if (!t) title = '문서 없음';
  else title = t.dirty ? `${t.title} — 편집됨` : t.title;
  // macOS 한정으로 표시되는 .app-titlebar 한가운데에 현재 활성 탭 제목과
  // 편집됨 표시(`is-dirty` → CSS ::after 가 " · 편집됨" 첨부)를 갱신한다.
  // 비-macOS 에선 같은 요소가 display:none 이라 갱신해도 무해.
  const barEl = document.getElementById('app-titlebar-title');
  if (barEl) {
    barEl.textContent = t ? t.title : '문서 없음';
    barEl.classList.toggle('is-dirty', !!(t && t.dirty));
  }
  if (title === lastWindowTitle) return;
  lastWindowTitle = title;
  try { document.title = title; } catch (_) {}
  // Drive the native OS titlebar via a Rust command so we bypass any
  // JS-side permission issues with window.setTitle.
  invoke('set_window_title', { title }).catch(() => null);
}

function addTab({ title, committed: c = '', path = null, savedCommitted } = {}) {
  commitTabState();
  const id = nextTabId++;
  const saved = typeof savedCommitted === 'string' ? savedCommitted : c;
  const tab = {
    id,
    title: title || `문서 ${id}`,
    path,
    committed: c,
    savedCommitted: saved,
    cursor: c.length,
    dirty: c !== saved,
  };
  tabs.push(tab);
  loadTabState(id);
  saveTabsDebounced();
  return tab;
}

async function closeTab(id) {
  const t = tabs.find((x) => x.id === id);
  if (!t) return;
  if (t.dirty) {
    const answer = await askConfirm({
      title: `"${t.title}" 에 저장되지 않은 변경 사항이 있습니다`,
      body: t.path
        ? '디스크에 저장하고 닫을까요?'
        : '새 파일로 저장하거나 변경 사항을 버리고 닫을 수 있습니다.',
    });
    if (answer === 'cancel') return;
    if (answer === 'save') {
      const ok = await saveTab(t);
      if (!ok) return;
    }
  }
  const idx = tabs.findIndex((x) => x.id === id);
  tabs.splice(idx, 1);
  if (!tabs.length) {
    addTab({});
    return;
  }
  if (id === activeTabId) {
    const nextIdx = Math.min(idx, tabs.length - 1);
    loadTabState(tabs[nextIdx].id);
  } else {
    renderTabs();
  }
  saveTabsDebounced();
}

async function saveTab(t) {
  try {
    if (t.path) {
      await invoke('write_markdown_file', { path: t.path, content: t.committed });
    } else {
      const savedPath = await invoke('save_as_markdown', {
        suggestedName: t.title.endsWith('.md') ? t.title : `${t.title}.md`,
        content: t.committed,
      });
      if (!savedPath) return false;
      t.path = savedPath;
      // Use file's basename as title.
      t.title = savedPath.split('/').pop() || savedPath.split('\\').pop() || t.title;
    }
    t.savedCommitted = t.committed;
    t.dirty = false;
    renderTabs();
    // 상단 제목표시줄 · 네이티브 타이틀에서 "편집됨" 표시를 즉시 제거.
    updateWindowTitle();
    saveTabsDebounced();
    return true;
  } catch (e) {
    logEvent(`저장 실패: ${e}`);
    return false;
  }
}

async function saveActiveTab() {
  commitTabState();
  const t = activeTab();
  if (!t) return;
  if (!t.dirty && t.path) { logEvent('변경 없음'); return; }
  await saveTab(t);
  logEvent(`저장 완료: ${t.title}`);
}

document.getElementById('tab-new')?.addEventListener('click', () => {
  addTab({});
  editor.focus();
});

/* ═══════════════════════ Files pane ═══════════════════════ */
const filesOpenBtn = document.getElementById('files-open');
const filesRefreshBtn = document.getElementById('files-refresh');
const filesListEl = document.getElementById('files-list');
const filesDirEl = document.getElementById('files-dir');
const filesEmptyEl = document.getElementById('files-empty');
let rootDir = null;                       // root of the tree view
const expandedDirs = new Set();           // paths currently expanded
const dirCache = new Map();               // path → entries[] (lazy)

function pathParent(p) {
  if (!p) return null;
  const sep = p.includes('\\') && !p.includes('/') ? '\\' : '/';
  const idx = p.lastIndexOf(sep);
  if (idx <= 0) return null;
  const parent = p.slice(0, idx);
  return parent || sep;
}

async function openDirectory() {
  statusMessage('디렉터리 선택 중…');
  let dir = null;
  try {
    dir = await invoke('pick_directory');
  } catch (e) {
    statusMessage(`디렉터리 열기 실패: ${e}`);
    return;
  }
  if (!dir) { statusMessage(''); return; }
  rootDir = dir;
  expandedDirs.clear();
  dirCache.clear();
  try { localStorage.setItem(FILES_DIR_KEY, dir); } catch (_) {}
  statusMessage(`열기: ${dir}`);
  try {
    await refreshFileList();
  } catch (e) {
    statusMessage(`트리 갱신 실패: ${e}`);
  }
  // 디렉터리를 새로 열면 개요·문서·검색 뷰 중 무엇이 떠 있었든 파일
  // 트리로 전환해 방금 연 루트를 바로 볼 수 있게 한다.
  if (typeof switchSidebarView === 'function') switchSidebarView('files');
}

function statusMessage(msg) {
  const el = document.getElementById('status-path');
  if (el) el.textContent = msg || '';
  if (msg) logEvent(msg);
}

async function refreshFileList() {
  if (!rootDir) return;
  if (filesDirEl) {
    filesDirEl.hidden = false;
    filesDirEl.classList.remove('files-crumbs');
    const base = rootDir.split(/[\\/]/).filter(Boolean).pop() || rootDir;
    filesDirEl.textContent = base;
    filesDirEl.title = rootDir; // full path on hover tooltip
    filesDirEl.style.direction = 'ltr';
    filesDirEl.style.textAlign = 'left';
  }
  if (filesRefreshBtn) filesRefreshBtn.hidden = false;
  dirCache.clear(); // re-fetch on manual refresh
  let entries;
  try {
    entries = await listDir(rootDir);
  } catch (e) {
    filesEmptyEl.textContent = `디렉터리를 읽을 수 없습니다: ${e}`;
    filesEmptyEl.hidden = false;
    filesListEl.innerHTML = '';
    return;
  }
  renderTree(entries);
  // 트리가 바뀌면 에디터의 wiki-link 존재 여부도 다시 검사.
  if (typeof refreshWikilinkStates === 'function') refreshWikilinkStates();
}

// 트리가 갱신되면 현재 문서에 있는 [[...]] 위키링크들의 존재 여부 표시를
// 다시 평가한다 (새 파일이 생겼거나 삭제됐을 수 있음).
function refreshWikilinkStates() {
  if (!markdownMode || !editor) return;
  const spans = editor.querySelectorAll('.md-wikilink');
  spans.forEach((w) => {
    w.classList.remove('md-wikilink-exists', 'md-wikilink-missing');
    delete w.dataset.resolved;
    markWikilinkExistence(w, w.dataset.name || '');
  });
}

async function listDir(path) {
  if (dirCache.has(path)) return dirCache.get(path);
  const entries = await invoke('list_markdown_files', { path });
  dirCache.set(path, entries);
  return entries;
}

async function toggleFolder(path) {
  if (expandedDirs.has(path)) {
    expandedDirs.delete(path);
  } else {
    expandedDirs.add(path);
    try { await listDir(path); } catch (e) {
      expandedDirs.delete(path);
      return;
    }
  }
  const rootEntries = dirCache.get(rootDir) || [];
  renderTree(rootEntries);
}

const FOLDER_ICON = '<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M2 4.5a1 1 0 0 1 1-1h3.5l1.5 1.5H13a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V4.5z"/></svg>';
const FILE_ICON = '<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 1.5h6L13 5v9a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V2.5a1 1 0 0 1 1-1z"/><polyline points="9,1.5 9,5 13,5"/></svg>';

function renderTree(rootEntries) {
  filesListEl.innerHTML = '';
  if (!rootEntries.length) {
    filesEmptyEl.textContent = '이 디렉터리에 하위 폴더나 마크다운 파일(.md) 이 없습니다.';
    filesEmptyEl.hidden = false;
    return;
  }
  filesEmptyEl.hidden = true;
  renderEntries(rootEntries, 0);
}

function renderEntries(entries, depth) {
  for (const f of entries) {
    const li = createFileItemEl(f, depth);
    filesListEl.appendChild(li);
    if (f.kind === 'dir' && expandedDirs.has(f.path) && dirCache.has(f.path)) {
      renderEntries(dirCache.get(f.path), depth + 1);
    }
  }
}

function createFileItemEl(f, depth) {
  const li = document.createElement('li');
  const isDir = f.kind === 'dir';
  const isExpanded = isDir && expandedDirs.has(f.path);
  const openTab = !isDir && tabs.find((t) => t.path === f.path);
  const isPinned = pinnedPaths.has(f.path);
  li.className = 'file-item'
    + (isDir ? ' file-item-dir' : '')
    + (isExpanded ? ' expanded' : '')
    + (openTab && openTab.id === activeTabId ? ' active' : '')
    + (isPinned ? ' pinned' : '');
  li.dataset.path = f.path;
  li.dataset.kind = f.kind;
  li.dataset.name = f.name;
  li.tabIndex = -1;
  li.style.setProperty('--depth', String(depth));
  li.addEventListener('contextmenu', (ev) => openFileCtxMenu(ev, f));

  const caret = document.createElement('span');
  caret.className = 'file-caret';
  if (isDir) {
    caret.innerHTML = '<svg width="9" height="9" viewBox="0 0 10 10" fill="currentColor"><path d="M3 2l4 3-4 3z"/></svg>';
  }
  li.appendChild(caret);

  const ico = document.createElement('span');
  ico.className = 'file-ico';
  ico.innerHTML = isDir ? FOLDER_ICON : FILE_ICON;
  const name = document.createElement('span');
  name.className = 'file-name';
  name.textContent = f.name;
  li.append(ico, name);

  li.addEventListener('click', async () => {
    selectFileItem(li);
    if (isDir) await toggleFolder(f.path);
    else await openFile(f.path, f.name);
  });
  return li;
}

let selectedFileEl = null;
function selectFileItem(li) {
  if (selectedFileEl && selectedFileEl !== li) selectedFileEl.classList.remove('selected');
  selectedFileEl = li;
  if (li) { li.classList.add('selected'); li.focus(); }
}

/* Right-click on files-pane background (not an item) — offer create. */
(function bindFilesBgCtxMenu() {
  const pane = document.getElementById('files-pane');
  if (!pane) return;
  pane.addEventListener('contextmenu', (ev) => {
    if (ev.target.closest('.file-item')) return; // item handler took over
    if (!rootDir) return;                         // no dir open — nothing to add to
    ev.preventDefault();
    ev.stopPropagation();
    openRootCtxMenu(ev, rootDir);
  });
})();

function openRootCtxMenu(ev, dir) {
  const actions = [
    { label: '새 파일', act: 'new-file' },
    { label: '새 폴더', act: 'new-dir' },
  ];
  fileCtxMenu.innerHTML = '';
  for (const it of actions) {
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = it.label;
    b.addEventListener('click', async (e) => {
      e.stopPropagation();
      hideFileCtxMenu();
      await runRootAction(it.act, dir);
    });
    fileCtxMenu.appendChild(b);
  }
  fileCtxMenu.classList.remove('hidden');
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const r = fileCtxMenu.getBoundingClientRect();
  const x = Math.min(ev.clientX, vw - r.width - 8);
  const y = Math.min(ev.clientY, vh - r.height - 8);
  fileCtxMenu.style.left = `${Math.max(8, x)}px`;
  fileCtxMenu.style.top = `${Math.max(8, y)}px`;
}

async function runRootAction(act, dir) {
  const sep = dir.includes('\\') && !dir.includes('/') ? '\\' : '/';
  try {
    if (act === 'new-file') {
      const name = await askPrompt({
        title: '새 파일',
        body: `${dir} 에 생성할 파일 이름을 입력하세요.`,
        defaultValue: 'untitled.md',
        placeholder: 'note.md',
        confirmLabel: '생성',
      });
      if (!name) return;
      const p = `${dir}${sep}${name}`;
      await invoke('fs_create_file', { path: p });
      await invalidateDirAndRefresh(dir);
      await openFile(p, name);
    } else if (act === 'new-dir') {
      const name = await askPrompt({
        title: '새 폴더',
        body: `${dir} 에 생성할 폴더 이름을 입력하세요.`,
        defaultValue: '새 폴더',
        confirmLabel: '생성',
      });
      if (!name) return;
      const p = `${dir}${sep}${name}`;
      await invoke('fs_create_dir', { path: p });
      await invalidateDirAndRefresh(dir);
    }
  } catch (e) {
    logEvent(`생성 실패: ${e && e.message || e}`);
  }
}

/* Keyboard: Enter = rename selected, Cmd/Ctrl+Backspace = delete */
document.addEventListener('keydown', (ev) => {
  const sel = selectedFileEl;
  if (!sel || !document.activeElement || !document.activeElement.classList.contains('file-item')) return;
  const path = sel.dataset.path;
  const name = sel.dataset.name;
  const kind = sel.dataset.kind;
  const parent = pathParent(path);
  const entry = { path, name, isDir: kind === 'dir', parent };
  if (ev.key === 'Enter' && !ev.metaKey && !ev.ctrlKey && !ev.shiftKey && !ev.altKey) {
    ev.preventDefault();
    runFileAction('rename', entry);
  } else if ((ev.key === 'Backspace' || ev.key === 'Delete') && (ev.metaKey || ev.ctrlKey)) {
    ev.preventDefault();
    runFileAction('delete', entry);
  } else if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
    ev.preventDefault();
    const items = Array.from(document.querySelectorAll('#files-list .file-item'));
    const idx = items.indexOf(sel);
    const next = items[Math.max(0, Math.min(items.length - 1, idx + (ev.key === 'ArrowDown' ? 1 : -1)))];
    if (next) selectFileItem(next);
  }
});

/* ─────────── Wiki-link ([[문서 이름]]) — 내부 문서 링크 ───────────
 * • 현재 열려 있는 폴더 트리(dirCache)에서 "<이름>.md" 와 대소문자 무시로
 *   일치하는 파일을 찾아 존재 여부에 따라 두 가지 스타일로 렌더.
 * • ⌘(Ctrl)+클릭 — 있으면 그 파일을 탭으로 열고, 없으면 "만들까요?"
 *   확인 후 현재 문서의 폴더(또는 루트) 에 빈 파일로 생성하고 연다.
 * • dirCache 는 lazy 라 펼쳐 본 적 없는 하위 폴더의 파일은 보이지 않을
 *   수 있다. 그 경우 '없음' 으로 표시되어도 생성 프롬프트에서 실제 경로
 *   이름 충돌을 Rust 쪽에서 한번 더 잡아 준다. */

function resolveWikiLink(name) {
  if (!name) return null;
  const target = String(name).trim().toLowerCase();
  if (!target) return null;
  for (const [, entries] of dirCache) {
    if (!Array.isArray(entries)) continue;
    for (const e of entries) {
      if (!e || e.kind !== 'file') continue;
      const base = String(e.name || '').replace(/\.md$/i, '');
      if (base.toLowerCase() === target) return e.path;
    }
  }
  return null;
}

function markWikilinkExistence(span, name) {
  const path = resolveWikiLink(name);
  if (path) {
    span.classList.add('md-wikilink-exists');
    span.dataset.resolved = path;
  } else {
    span.classList.add('md-wikilink-missing');
  }
}

async function createWikilinkDocument(name) {
  // 생성 위치: 현재 활성 탭의 디렉터리 → 없으면 루트. 둘 다 없으면 중단.
  const active = (typeof activeTab === 'function') ? activeTab() : null;
  let dir = null;
  if (active && active.path) {
    dir = active.path.replace(/[\\/][^\\/]+$/, '');
  }
  if (!dir && rootDir) dir = rootDir;
  if (!dir) {
    logEvent('문서 생성 실패: 먼저 폴더를 여세요');
    return;
  }
  const safeName = String(name).trim().replace(/[\\/:*?"<>|]/g, '_');
  if (!safeName) return;
  const sep = dir.includes('\\') && !dir.includes('/') ? '\\' : '/';
  const fullPath = `${dir}${sep}${safeName}.md`;
  try {
    await invoke('fs_create_file', { path: fullPath });
  } catch (e) {
    // 이미 존재하면 그냥 열면 된다 — 경쟁 상태로 해석.
    const msg = String(e || '');
    if (!/이미 존재/.test(msg)) {
      logEvent(`문서 생성 실패: ${msg}`);
      return;
    }
  }
  // 트리 캐시 무효화 + 트리 새로고침 + 열기.
  dirCache.delete(dir);
  try { if (typeof refreshFileList === 'function') await refreshFileList(); } catch (_) {}
  await openFile(fullPath, `${safeName}.md`);
  logEvent(`새 문서: ${safeName}.md`);
}

async function handleWikilinkClick(ev) {
  const wrap = ev.target && ev.target.closest && ev.target.closest('.md-wikilink');
  if (!wrap) return;
  if (!(ev.metaKey || ev.ctrlKey)) return;
  ev.preventDefault();
  ev.stopPropagation();
  const name = wrap.dataset.name || '';
  if (!name.trim()) return;
  const resolved = wrap.dataset.resolved || resolveWikiLink(name);
  if (resolved) {
    await openFile(resolved, `${name}.md`);
    return;
  }
  const answer = await askConfirm({
    title: `"${name}" 문서를 새로 만들까요?`,
    body: rootDir
      ? `현재 폴더에 해당 문서가 없어요. 빈 "${name}.md" 를 만들고 엽니다.`
      : `아직 작업 폴더가 지정되지 않았어요. 먼저 "파일 → 폴더 열기" 로 폴더를 여세요.`,
    saveLabel: '만들기',
    discardLabel: '',
    cancelLabel: '취소',
    showSave: !!rootDir,
  });
  if (answer !== 'save') return;
  await createWikilinkDocument(name);
}

async function openFile(path, name) {
  const existing = tabs.find((t) => t.path === path);
  if (existing) {
    commitTabState();
    loadTabState(existing.id);
    return;
  }
  let content = '';
  try {
    content = await invoke('read_markdown_file', { path });
  } catch (e) {
    logEvent(`열기 실패: ${e}`);
    return;
  }
  addTab({ title: name || path.split('/').pop(), path, committed: content, savedCommitted: content });
  if (filesListEl) refreshFileActiveState();
  if (!markdownMode) {
    markdownMode = true;
    if (mdToggle) mdToggle.checked = true;
    try { localStorage.setItem(MD_KEY, '1'); } catch (_) {}
    render();
  }
}

function refreshFileActiveState() {
  if (!filesListEl) return;
  const active = activeTab();
  filesListEl.querySelectorAll('.file-item').forEach((el) => {
    el.classList.toggle('active', !!active && el.dataset.path === active.path);
  });
}

filesOpenBtn?.addEventListener('click', openDirectory);
filesRefreshBtn?.addEventListener('click', refreshFileList);

/* ─────────── Files pane collapse ─────────── */
const FILES_COLLAPSED_KEY = 'leaf-ime:files-collapsed';
(function () {
  const btn = document.getElementById('files-collapse');
  const pane = document.getElementById('files-pane');
  if (!btn || !pane) return;
  const apply = (v) => pane.classList.toggle('collapsed', !!v);
  try { apply(localStorage.getItem(FILES_COLLAPSED_KEY) === '1'); } catch {}
  btn.addEventListener('click', () => {
    const nowCollapsed = !pane.classList.contains('collapsed');
    apply(nowCollapsed);
    try { localStorage.setItem(FILES_COLLAPSED_KEY, nowCollapsed ? '1' : '0'); } catch {}
    reCenterStyleBar();
  });
})();

/* ─────────── Pinned files (persistent per root) ─────────── */
const PIN_KEY = 'leaf-ime:pinned-files';
function loadPinned() {
  try { return new Set(JSON.parse(localStorage.getItem(PIN_KEY) || '[]')); }
  catch { return new Set(); }
}
function savePinned(set) {
  try { localStorage.setItem(PIN_KEY, JSON.stringify([...set])); } catch {}
}
const pinnedPaths = loadPinned();

/* ─────────── Files context menu ─────────── */
const fileCtxMenu = document.getElementById('file-ctxmenu');

function openFileCtxMenu(ev, entry) {
  ev.preventDefault();
  ev.stopPropagation();
  const isDir = entry.kind === 'dir';
  const path = entry.path;
  const name = entry.name;
  const parent = pathParent(path);
  const isPinned = pinnedPaths.has(path);

  const actions = [];
  actions.push({ label: '새 파일', kind: 'normal', act: 'new-file' });
  actions.push({ label: '새 폴더', kind: 'normal', act: 'new-dir' });
  actions.push({ sep: true });
  actions.push({ label: '이름 변경', kind: 'normal', act: 'rename' });
  if (!isDir) actions.push({ label: '복제', kind: 'normal', act: 'duplicate' });
  actions.push({ label: isPinned ? '핀 해제' : '핀 고정', kind: 'normal', act: 'pin' });
  actions.push({ sep: true });
  actions.push({ label: isDir ? '이 위치를 루트로' : '이 파일 위치로', kind: 'normal', act: 'explore' });
  actions.push({ label: 'Finder에서 보기', kind: 'normal', act: 'reveal' });
  actions.push({ label: '경로 복사', kind: 'normal', act: 'copy-path' });
  actions.push({ sep: true });
  actions.push({ label: '삭제', kind: 'danger', act: 'delete' });

  fileCtxMenu.innerHTML = '';
  for (const item of actions) {
    if (item.sep) {
      const s = document.createElement('div');
      s.className = 'ctx-sep';
      fileCtxMenu.appendChild(s);
      continue;
    }
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = item.label;
    if (item.kind === 'danger') b.classList.add('danger');
    b.addEventListener('click', (e) => {
      e.stopPropagation();
      hideFileCtxMenu();
      runFileAction(item.act, { path, name, isDir, parent });
    });
    fileCtxMenu.appendChild(b);
  }
  fileCtxMenu.classList.remove('hidden');
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const r = fileCtxMenu.getBoundingClientRect();
  const x = Math.min(ev.clientX, vw - r.width - 8);
  const y = Math.min(ev.clientY, vh - r.height - 8);
  fileCtxMenu.style.left = `${Math.max(8, x)}px`;
  fileCtxMenu.style.top = `${Math.max(8, y)}px`;
}

function hideFileCtxMenu() {
  fileCtxMenu?.classList.add('hidden');
}
document.addEventListener('click', (ev) => {
  if (!fileCtxMenu || fileCtxMenu.classList.contains('hidden')) return;
  if (!ev.target.closest('#file-ctxmenu')) hideFileCtxMenu();
});
window.addEventListener('keydown', (ev) => {
  if (ev.key === 'Escape' && fileCtxMenu && !fileCtxMenu.classList.contains('hidden')) {
    ev.preventDefault();
    hideFileCtxMenu();
  }
}, true);
window.addEventListener('blur', hideFileCtxMenu);

async function runFileAction(act, ctx) {
  try {
    switch (act) {
      case 'new-file': {
        const parentDir = ctx.isDir ? ctx.path : ctx.parent;
        const name = await askPrompt({
          title: '새 파일',
          body: '생성할 파일 이름을 입력하세요.',
          defaultValue: 'untitled.md',
          placeholder: 'note.md',
          confirmLabel: '생성',
        });
        if (!name) return;
        const sep = parentDir.includes('\\') && !parentDir.includes('/') ? '\\' : '/';
        const newPath = `${parentDir}${sep}${name}`;
        await invoke('fs_create_file', { path: newPath });
        await invalidateDirAndRefresh(parentDir);
        await openFile(newPath, name);
        break;
      }
      case 'new-dir': {
        const parentDir = ctx.isDir ? ctx.path : ctx.parent;
        const name = await askPrompt({
          title: '새 폴더',
          body: '생성할 폴더 이름을 입력하세요.',
          defaultValue: '새 폴더',
          confirmLabel: '생성',
        });
        if (!name) return;
        const sep = parentDir.includes('\\') && !parentDir.includes('/') ? '\\' : '/';
        const newPath = `${parentDir}${sep}${name}`;
        await invoke('fs_create_dir', { path: newPath });
        await invalidateDirAndRefresh(parentDir);
        break;
      }
      case 'rename': {
        const newName = await askPrompt({
          title: '이름 변경',
          body: `"${ctx.name}" 의 새 이름을 입력하세요.`,
          defaultValue: ctx.name,
          confirmLabel: '변경',
        });
        if (!newName || newName === ctx.name) return;
        const sep = ctx.parent.includes('\\') && !ctx.parent.includes('/') ? '\\' : '/';
        const newPath = `${ctx.parent}${sep}${newName}`;
        await invoke('fs_rename', { oldPath: ctx.path, newPath });
        // Update any open tab pointing at the old path.
        const tab = tabs.find((t) => t.path === ctx.path);
        if (tab) { tab.path = newPath; tab.title = newName; }
        if (pinnedPaths.delete(ctx.path)) { pinnedPaths.add(newPath); savePinned(pinnedPaths); }
        await invalidateDirAndRefresh(ctx.parent);
        renderTabs();
        updateWindowTitle();
        break;
      }
      case 'duplicate': {
        const copied = await invoke('fs_duplicate_file', { path: ctx.path });
        await invalidateDirAndRefresh(ctx.parent);
        logEvent(`복제: ${copied}`);
        break;
      }
      case 'pin': {
        if (pinnedPaths.has(ctx.path)) pinnedPaths.delete(ctx.path);
        else pinnedPaths.add(ctx.path);
        savePinned(pinnedPaths);
        // Re-render tree so pinned items show the star.
        const rootEntries = dirCache.get(rootDir) || [];
        renderTree(sortWithPinned(rootEntries));
        break;
      }
      case 'explore': {
        if (ctx.isDir) {
          rootDir = ctx.path;
          expandedDirs.clear();
          dirCache.clear();
          try { localStorage.setItem(FILES_DIR_KEY, rootDir); } catch {}
          await refreshFileList();
        } else {
          rootDir = ctx.parent;
          expandedDirs.clear();
          dirCache.clear();
          try { localStorage.setItem(FILES_DIR_KEY, rootDir); } catch {}
          await refreshFileList();
        }
        break;
      }
      case 'reveal': {
        await invoke('fs_reveal', { path: ctx.path });
        break;
      }
      case 'copy-path': {
        try {
          await navigator.clipboard.writeText(ctx.path);
          logEvent(`경로 복사: ${ctx.path}`);
        } catch {
          // Fallback via Tauri clipboard plugin.
          try {
            const cm = window.__TAURI__?.clipboardManager;
            if (cm && cm.writeText) await cm.writeText(ctx.path);
          } catch {}
        }
        break;
      }
      case 'delete': {
        const answer = await askConfirm({
          title: `"${ctx.name}" 을(를) 삭제할까요?`,
          body: ctx.isDir
            ? '폴더와 그 안의 모든 내용이 디스크에서 삭제됩니다. 되돌릴 수 없습니다.'
            : '파일이 디스크에서 삭제됩니다. 되돌릴 수 없습니다.',
          saveLabel: '삭제',
          discardLabel: '',
          cancelLabel: '취소',
          showSave: true,
        });
        if (answer !== 'save') return;
        await invoke('fs_delete', { path: ctx.path });
        // Close the corresponding tab if open.
        const tab = tabs.find((t) => t.path === ctx.path);
        if (tab) {
          tab.savedCommitted = tab.committed; // force no-dirty
          tab.dirty = false;
          await closeTab(tab.id);
        }
        if (pinnedPaths.delete(ctx.path)) savePinned(pinnedPaths);
        await invalidateDirAndRefresh(ctx.parent);
        break;
      }
    }
  } catch (e) {
    logEvent(`동작 실패: ${e && e.message || e}`);
  }
}

async function invalidateDirAndRefresh(dirPath) {
  if (!dirPath) return;
  dirCache.delete(dirPath);
  // If the dirPath is the root or an expanded folder, re-render tree.
  if (dirPath === rootDir) {
    await refreshFileList();
  } else if (expandedDirs.has(dirPath)) {
    await listDir(dirPath);
    const rootEntries = dirCache.get(rootDir) || [];
    renderTree(sortWithPinned(rootEntries));
  } else {
    // The item was in a collapsed subtree — just clear its cache.
  }
}

function sortWithPinned(entries) {
  if (!pinnedPaths.size) return entries;
  return [...entries].sort((a, b) => {
    const ap = pinnedPaths.has(a.path) ? 0 : 1;
    const bp = pinnedPaths.has(b.path) ? 0 : 1;
    if (ap !== bp) return ap - bp;
    return 0;
  });
}

/* ═══════════════════════ Auto-save tab state on every render ═══════════════════════ */
const origRenderCore = renderCore;
renderCore = function patchedRenderCore(...args) {
  origRenderCore.apply(this, args);
  commitTabState();
  saveTabsDebounced();
  renderTabs();
  refreshFileActiveState();
  updateStatusBar();
  updateStyleBar();
  ensureCaretVisible();
  renderMermaidBlocks();    // async — injects SVG previews after the DOM is built
  highlightCodeBlocks();    // async — paints tokens inside fenced code lines
  renderMathBlocks();       // async — KaTeX 수식 프리뷰
  renderTocBlocks();        // sync  — [TOC] 라인 아래에 제목 리스트
};

/* ═══════════════════════ Mermaid rendering ═══════════════════════
   Lazy-loaded ESM bundle from jsdelivr. SVGs are cached by source so
   unrelated keystrokes don't trigger re-renders. The preview element
   is injected as a sibling of the closing fence with class
   `.md-mermaid-preview` — since our cursor walkers only consider
   `.md-line` blocks, previews never contribute to source-index math.
*/
let mermaidLoader = null;
const mermaidCache = new Map(); // code+theme → svg html
let mermaidRenderId = 0;
let mermaidIdCounter = 0;

function currentMermaidTheme() {
  const t = document.documentElement.dataset.theme;
  if (t === 'moss' || t === 'arctic' || t === 'sepia') return 'default';
  return 'dark';
}

async function ensureMermaid() {
  if (mermaidLoader) return mermaidLoader;
  mermaidLoader = (async () => {
    try {
      const mod = await import('https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs');
      const mermaid = mod.default;
      mermaid.initialize({
        startOnLoad: false,
        theme: currentMermaidTheme(),
        securityLevel: 'loose',
        flowchart: { useMaxWidth: true, htmlLabels: true },
        themeVariables: { fontFamily: "'Noto Sans KR', -apple-system, sans-serif" },
      });
      // After mermaid is ready, trigger another render so any existing
      // mermaid blocks in the document get their preview attached.
      setTimeout(() => render(), 0);
      return mermaid;
    } catch (e) {
      console.warn('mermaid load failed:', e);
      return null;
    }
  })();
  return mermaidLoader;
}

// Re-initialize mermaid + clear cache on theme change so diagrams
// repaint in the new palette.
const origApplyTheme = applyTheme;
applyTheme = function themedApplyTheme(name) {
  origApplyTheme(name);
  if (mermaidLoader) {
    mermaidCache.clear();
    mermaidLoader.then((m) => {
      if (m) {
        m.initialize({
          startOnLoad: false,
          theme: currentMermaidTheme(),
          securityLevel: 'loose',
          flowchart: { useMaxWidth: true, htmlLabels: true },
          themeVariables: { fontFamily: "'Noto Sans KR', -apple-system, sans-serif" },
        });
        render();
      }
    });
  }
};

function findMermaidBlocks() {
  const out = [];
  const opens = editor.querySelectorAll('.md-fence-open[data-lang="mermaid"]');
  for (const open of opens) {
    const codeLines = [];
    let n = open.nextElementSibling;
    while (n && !n.classList.contains('md-fence-close')) {
      if (n.classList.contains('md-code-line')) codeLines.push(n.textContent);
      n = n.nextElementSibling;
    }
    if (!n) continue; // unclosed fence — skip
    out.push({ open, close: n, code: codeLines.join('\n') });
  }
  return out;
}

async function renderMermaidBlocks() {
  if (!markdownMode) return;
  const blocks = findMermaidBlocks();
  if (!blocks.length) return;
  const myId = ++mermaidRenderId;
  const mermaid = await ensureMermaid();
  if (myId !== mermaidRenderId) return;
  if (!mermaid) {
    for (const { close } of blocks) attachMermaidError(close, 'mermaid 로드 실패 (네트워크 확인)');
    return;
  }
  const theme = currentMermaidTheme();
  for (const { close, code } of blocks) {
    if (!close.isConnected) continue;
    const trimmed = code.trim();
    if (!trimmed) { attachMermaidError(close, ''); continue; }
    const key = `${theme}|${trimmed}`;
    let svgHtml = mermaidCache.get(key);
    if (!svgHtml) {
      try {
        const id = `mm-${++mermaidIdCounter}`;
        const result = await mermaid.render(id, trimmed);
        if (myId !== mermaidRenderId) return;
        svgHtml = result && result.svg ? result.svg : '';
        mermaidCache.set(key, svgHtml);
      } catch (e) {
        attachMermaidError(close, String(e && e.message || e));
        continue;
      }
    }
    if (!close.isConnected) continue;
    attachMermaidPreview(close, svgHtml);
  }
}

function attachMermaidPreview(afterNode, svgHtml) {
  const existing = afterNode.nextElementSibling;
  if (existing && existing.classList && existing.classList.contains('md-mermaid-preview')) {
    if (existing.dataset.svgKey !== svgHtml.length + '') {
      existing.innerHTML = svgHtml;
      existing.dataset.svgKey = svgHtml.length + '';
    }
    return existing;
  }
  const div = document.createElement('div');
  div.className = 'md-mermaid-preview';
  div.contentEditable = 'false';
  div.setAttribute('aria-hidden', 'true');
  div.innerHTML = svgHtml;
  div.dataset.svgKey = svgHtml.length + '';
  afterNode.parentNode.insertBefore(div, afterNode.nextSibling);
  return div;
}

function attachMermaidError(afterNode, message) {
  const existing = afterNode.nextElementSibling;
  const html = message
    ? `<div class="md-mermaid-err-text">${escapeTextForHtml(message)}</div>`
    : '<div class="md-mermaid-err-text">empty diagram</div>';
  if (existing && existing.classList && existing.classList.contains('md-mermaid-preview')) {
    existing.className = 'md-mermaid-preview md-mermaid-error';
    existing.innerHTML = html;
    return existing;
  }
  const div = document.createElement('div');
  div.className = 'md-mermaid-preview md-mermaid-error';
  div.contentEditable = 'false';
  div.setAttribute('aria-hidden', 'true');
  div.innerHTML = html;
  afterNode.parentNode.insertBefore(div, afterNode.nextSibling);
  return div;
}

function escapeTextForHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}

/* ═══════════════════════ Math ($$ … $$) via KaTeX ═══════════════════════
   mermaid 와 같은 lazy-load / async 렌더 패턴. 소스 라인은 그대로 DOM 에
   남기고 닫는 $$ 다음에 contenteditable=false 프리뷰 <div> 를 꽂는다.
*/
let katexLoader = null;
const katexCache = new Map(); // latex → innerHTML
let mathRenderId = 0;

async function ensureKatex() {
  if (katexLoader) return katexLoader;
  katexLoader = (async () => {
    try {
      // CSS 는 한 번만 주입 (에디터 테마와 섞이지 않도록 scope 없이 그대로).
      if (!document.querySelector('link[data-katex-css]')) {
        const link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = 'https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.css';
        link.dataset.katexCss = '1';
        document.head.appendChild(link);
      }
      const mod = await import('https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.mjs');
      const katex = mod.default || mod;
      // KaTeX 가 로드된 뒤 기존 블록들이 그려지도록 한 번 더 render.
      setTimeout(() => render(), 0);
      return katex;
    } catch (e) {
      console.warn('katex load failed:', e);
      return null;
    }
  })();
  return katexLoader;
}

function findMathBlocks() {
  const out = [];
  const opens = editor.querySelectorAll('.md-math-open');
  for (const open of opens) {
    const bodies = [];
    let n = open.nextElementSibling;
    while (n && !n.classList.contains('md-math-close')) {
      if (n.classList.contains('md-math-body')) bodies.push(n.textContent);
      n = n.nextElementSibling;
    }
    if (!n) continue; // 미종결
    out.push({ open, close: n, latex: bodies.join('\n') });
  }
  return out;
}

async function renderMathBlocks() {
  if (!markdownMode) return;
  const blocks = findMathBlocks();
  // 단일 라인 $$...$$ 블록도 동일 렌더 파이프라인으로 처리. `close` 역할은
  // 자기 자신(= 프리뷰를 붙일 기준 노드) 이 맡고, `latex` 는 data-latex.
  const singles = Array.from(editor.querySelectorAll('.md-math-single')).map((el) => ({
    close: el, latex: el.dataset.latex || '',
  }));
  const all = [...blocks, ...singles];
  if (!all.length) return;
  const myId = ++mathRenderId;
  const katex = await ensureKatex();
  if (myId !== mathRenderId) return;
  if (!katex) {
    for (const { close } of all) attachMathError(close, 'KaTeX 로드 실패 (네트워크 확인)');
    return;
  }
  for (const { close, latex } of all) {
    if (!close.isConnected) continue;
    const src = latex.trim();
    if (!src) { detachMathPreview(close); continue; }
    let html = katexCache.get(src);
    if (!html) {
      try {
        html = katex.renderToString(src, {
          displayMode: true,
          throwOnError: false,
          strict: 'ignore',
        });
        katexCache.set(src, html);
      } catch (e) {
        attachMathError(close, String(e && e.message || e));
        continue;
      }
    }
    attachMathPreview(close, html);
  }
}

function attachMathPreview(afterNode, html) {
  const existing = afterNode.nextElementSibling;
  if (existing && existing.classList && existing.classList.contains('md-math-preview')) {
    if (existing.dataset.latexKey !== String(html.length)) {
      existing.innerHTML = html;
      existing.dataset.latexKey = String(html.length);
    }
    existing.classList.remove('md-math-error');
    return existing;
  }
  const div = document.createElement('div');
  div.className = 'md-math-preview';
  div.contentEditable = 'false';
  div.setAttribute('aria-hidden', 'true');
  div.innerHTML = html;
  div.dataset.latexKey = String(html.length);
  afterNode.parentNode.insertBefore(div, afterNode.nextSibling);
  return div;
}
function attachMathError(afterNode, message) {
  const existing = afterNode.nextElementSibling;
  const html = `<div class="md-mermaid-err-text">${escapeTextForHtml(message)}</div>`;
  if (existing && existing.classList && existing.classList.contains('md-math-preview')) {
    existing.className = 'md-math-preview md-math-error';
    existing.innerHTML = html;
    return existing;
  }
  const div = document.createElement('div');
  div.className = 'md-math-preview md-math-error';
  div.contentEditable = 'false';
  div.setAttribute('aria-hidden', 'true');
  div.innerHTML = html;
  afterNode.parentNode.insertBefore(div, afterNode.nextSibling);
  return div;
}
function detachMathPreview(afterNode) {
  const existing = afterNode.nextElementSibling;
  if (existing && existing.classList && existing.classList.contains('md-math-preview')) {
    existing.remove();
  }
}

/* ═══════════════════════ [TOC] 프리뷰 ═══════════════════════
   문서의 제목(h1-h6)들을 모아 TOC 라인 다음에 목록을 붙인다. 비동기
   자원을 쓰지 않으므로 render 직후에 바로 그릴 수 있다.
*/
function renderTocBlocks() {
  if (!markdownMode) return;
  const tocLines = editor.querySelectorAll('.md-line.md-toc');
  if (!tocLines.length) {
    // 남아있던 프리뷰 정리
    editor.querySelectorAll('.md-toc-preview').forEach((el) => el.remove());
    return;
  }
  const headings = editor.querySelectorAll('.md-h1, .md-h2, .md-h3, .md-h4, .md-h5, .md-h6');
  const items = [];
  for (const h of headings) {
    const lvl = parseInt(h.className.match(/md-h(\d)/)[1], 10);
    // 보이는 제목 텍스트 (md-syn prefix 제외)
    let title = '';
    for (const c of h.childNodes) {
      if (c.nodeType === Node.ELEMENT_NODE && c.classList && c.classList.contains('md-syn')) continue;
      title += c.textContent;
    }
    title = title.trim();
    if (!title) continue;
    items.push({ lvl, title });
  }
  for (const toc of tocLines) {
    attachTocPreview(toc, items);
  }
}
function attachTocPreview(afterNode, items) {
  const existing = afterNode.nextElementSibling;
  let div = (existing && existing.classList && existing.classList.contains('md-toc-preview'))
    ? existing
    : null;
  if (!div) {
    div = document.createElement('div');
    div.className = 'md-toc-preview';
    div.contentEditable = 'false';
    div.setAttribute('aria-hidden', 'true');
    afterNode.parentNode.insertBefore(div, afterNode.nextSibling);
  }
  if (!items.length) {
    div.innerHTML = '<div class="md-toc-empty">(제목이 아직 없음)</div>';
    return div;
  }
  const html = items.map(({ lvl, title }) => (
    `<div class="md-toc-item md-toc-l${lvl}">${escapeTextForHtml(title)}</div>`
  )).join('');
  div.innerHTML = `<div class="md-toc-title">목차</div>${html}`;
  return div;
}

/* ═══════════════════════ Syntax highlighting (highlight.js) ═══════════════════════
   Lazy ESM import from esm.sh which bundles highlight.js with the
   common language set (js / ts / py / json / rust / go / java / css /
   html / sql / bash / yaml / ...). Each fenced-code line is highlighted
   in place; textContent is preserved so cursor math keeps working.
*/
let hljsLoader = null;
const hljsLineCache = new Map(); // `${lang}|${line}` → highlighted html
let hljsRenderId = 0;

const HLJS_LANG_ALIAS = {
  js: 'javascript', ts: 'typescript', py: 'python', rb: 'ruby',
  sh: 'bash', zsh: 'bash', shell: 'bash', yml: 'yaml', md: 'markdown',
  'c++': 'cpp', rs: 'rust', kt: 'kotlin', cs: 'csharp', htm: 'xml',
  html: 'xml',
};

async function ensureHljs() {
  if (hljsLoader) return hljsLoader;
  hljsLoader = (async () => {
    try {
      const mod = await import('https://esm.sh/highlight.js@11.10.0');
      const hljs = mod.default || mod;
      // After load, re-render to populate code previews.
      setTimeout(() => render(), 0);
      return hljs;
    } catch (e) {
      console.warn('highlight.js load failed:', e);
      return null;
    }
  })();
  return hljsLoader;
}

function resolveHljsLang(hljs, lang) {
  if (!lang) return null;
  const raw = String(lang).trim().toLowerCase();
  const norm = HLJS_LANG_ALIAS[raw] || raw;
  if (hljs.getLanguage && hljs.getLanguage(norm)) return norm;
  return null;
}

async function highlightCodeBlocks() {
  if (!markdownMode) return;
  const codeLines = editor.querySelectorAll('.md-code-line[data-lang]:not([data-hl])');
  if (!codeLines.length) return;
  const myId = ++hljsRenderId;
  const hljs = await ensureHljs();
  if (myId !== hljsRenderId) return;
  if (!hljs) return;
  // Build a bucket of unique lang names we actually need so we can
  // skip work on unsupported ones quickly.
  const resolved = new Map();
  for (const line of codeLines) {
    const lang = line.dataset.lang;
    if (lang === 'mermaid') continue; // mermaid has its own preview path
    if (!resolved.has(lang)) resolved.set(lang, resolveHljsLang(hljs, lang));
  }
  for (const line of codeLines) {
    if (!line.isConnected) continue;
    // 커서가 놓인 라인을 하이라이팅하면 innerHTML 교체로 브라우저 selection
    // 이 잘려 "타이핑이 먹히지 않는" 현상이 생긴다. 커서가 다른 라인으로
    // 빠져나간 뒤에 따라잡아 칠하도록 data-hl 를 남기지 않고 건너뛴다.
    if (line.classList.contains('has-caret')) continue;
    const lang = line.dataset.lang;
    if (lang === 'mermaid') { line.dataset.hl = '1'; continue; }
    const resolvedLang = resolved.get(lang);
    if (!resolvedLang) { line.dataset.hl = '1'; continue; }
    const src = line.textContent;
    const key = `${resolvedLang}|${src}`;
    let html = hljsLineCache.get(key);
    if (!html) {
      try {
        html = hljs.highlight(src, { language: resolvedLang, ignoreIllegals: true }).value;
        hljsLineCache.set(key, html);
      } catch (_) {
        line.dataset.hl = '1';
        continue;
      }
    }
    // Replace the text node with the highlighted HTML. textContent still
    // equals src because hljs only wraps in <span class="hljs-...">.
    line.innerHTML = html;
    line.dataset.hl = '1';
  }
}

/* ═══════════════════════ Restore tabs + files on load ═══════════════════════ */
function restoreTabsAndFiles() {
  // Restore directory if any.
  try {
    const savedDir = localStorage.getItem(FILES_DIR_KEY);
    if (savedDir) { rootDir = savedDir; refreshFileList(); }
  } catch (_) {}
  // Restore tabs.
  try {
    const raw = localStorage.getItem(TABS_KEY);
    if (raw) {
      const d = JSON.parse(raw);
      if (d.tabs && d.tabs.length) {
        tabs = d.tabs.map((t) => ({ ...t, dirty: t.committed !== t.savedCommitted }));
        nextTabId = d.nextTabId || tabs.length + 1;
        const restoreId = d.activeTabId || tabs[0].id;
        loadTabState(restoreId);
        updateWindowTitle();
        return;
      }
    }
  } catch (_) {}
  // Fresh session — open a single empty tab.
  addTab({});
  updateWindowTitle();
}

/* ═══════════════════════ Save + close-window keybinds ═══════════════════════ */
window.addEventListener('keydown', (ev) => {
  if ((ev.metaKey || ev.ctrlKey) && ev.key === 's' && !ev.shiftKey) {
    ev.preventDefault();
    saveActiveTab();
  } else if ((ev.metaKey || ev.ctrlKey) && ev.key === 't' && !ev.shiftKey) {
    ev.preventDefault();
    addTab({});
    editor.focus();
  } else if ((ev.metaKey || ev.ctrlKey) && ev.key === 'w' && !ev.shiftKey) {
    ev.preventDefault();
    if (activeTabId != null) closeTab(activeTabId);
  } else if ((ev.metaKey || ev.ctrlKey) && ev.key === 'o' && !ev.shiftKey) {
    ev.preventDefault();
    openDirectory();
  } else if ((ev.metaKey || ev.ctrlKey) && ev.key === 'p' && !ev.shiftKey) {
    ev.preventDefault();
    doPrint();
  } else if ((ev.metaKey || ev.ctrlKey) && (ev.key === 'f' || ev.key === 'F') && !ev.shiftKey) {
    // Cmd/Ctrl+F — 에디터용 찾기·바꾸기 바 열기 (교체 섹션은 Cmd/Ctrl+Alt+F
    // 또는 바에 있는 ⇄ 버튼으로 펼침)
    ev.preventDefault();
    openFindBar(ev.altKey);
  } else if ((ev.metaKey || ev.ctrlKey) && ev.key === 'g') {
    // Cmd/Ctrl+G — 다음 / 이전 일치 (macOS 표준)
    ev.preventDefault();
    if (findBarOpen()) findNextMatch(ev.shiftKey ? -1 : 1);
  }
}, true);

/* ═══════════════════════ Window close — prompt on dirty tabs ═══════════════════════ */
window.addEventListener('beforeunload', (ev) => {
  commitTabState();
  if (tabs.some((t) => t.dirty)) {
    ev.preventDefault();
    ev.returnValue = '';
    return '';
  }
});
(async () => {
  try {
    const api = window.__TAURI__;
    const getWin =
      api?.webviewWindow?.getCurrentWebviewWindow
      || api?.window?.getCurrent;
    const w = getWin ? getWin() : null;
    if (w?.onCloseRequested) {
      await w.onCloseRequested(async (event) => {
        commitTabState();
        const dirty = tabs.filter((t) => t.dirty);
        if (!dirty.length) return;
        event.preventDefault();
        const answer = await askConfirm({
          title: '저장되지 않은 변경 사항',
          body: `${dirty.length}개의 탭에 저장되지 않은 변경 사항이 있습니다. 어떻게 할까요?`,
        });
        if (answer === 'cancel') return;
        if (answer === 'save') {
          for (const t of dirty) await saveTab(t);
        }
        w.destroy?.();
      });
    }
  } catch (_) {}
})();

/* ═══════════════════════ Markdown shortcut layer ═══════════════════════ */

// Returns the [start, end] range in `committed` for the current selection
// (collapsed → [cursor, cursor]). Null if selection isn't in the editor.
function mdRange() {
  const sel = window.getSelection();
  if (sel && sel.rangeCount && editor.contains(sel.anchorNode)) {
    const a = domToSourceIdx(sel.anchorNode, sel.anchorOffset);
    const b = domToSourceIdx(sel.focusNode, sel.focusOffset);
    if (typeof a === 'number' && typeof b === 'number') {
      return [Math.min(a, b), Math.max(a, b)];
    }
  }
  return [cursor, cursor];
}

function mdCurrentLineBounds(idx) {
  const before = committed.slice(0, idx);
  const ls = before.lastIndexOf('\n') + 1;
  const after = committed.slice(idx);
  const nextNl = after.indexOf('\n');
  const le = nextNl === -1 ? committed.length : idx + nextNl;
  return [ls, le];
}

async function mdFlushPreedit() {
  if (preedit) {
    const resp = await invoke('flush').catch(() => null);
    if (resp && resp.commit) {
      committed = committed.slice(0, cursor) + resp.commit + committed.slice(cursor);
      cursor += resp.commit.length;
    }
    preedit = '';
  }
}

async function mdWrap(left, right) {
  await mdFlushPreedit();
  const [s, e] = mdRange();
  snapshot();
  const sel = committed.slice(s, e);
  // Toggle: strip markers if selection is already wrapped.
  if (sel.startsWith(left) && sel.endsWith(right) && sel.length >= left.length + right.length) {
    const inner = sel.slice(left.length, sel.length - right.length);
    committed = committed.slice(0, s) + inner + committed.slice(e);
    cursor = s + inner.length;
  } else {
    const inserted = left + sel + right;
    committed = committed.slice(0, s) + inserted + committed.slice(e);
    if (s === e) cursor = s + left.length;
    else cursor = s + inserted.length;
  }
  render();
}

async function mdReplaceLinePrefix(fn) {
  await mdFlushPreedit();
  snapshot();
  const [ls, le] = mdCurrentLineBounds(cursor);
  const line = committed.slice(ls, le);
  const newLine = fn(line);
  committed = committed.slice(0, ls) + newLine + committed.slice(le);
  // Place cursor at (old cursor position adjusted by prefix length change).
  const delta = newLine.length - line.length;
  cursor = Math.max(ls, cursor + delta);
  render();
}

function mdToggleHeading(level) {
  return (line) => {
    const stripped = line.replace(/^#{1,6} /, '');
    if (level <= 0) return stripped;
    return '#'.repeat(level) + ' ' + stripped;
  };
}

function mdToggleQuote() {
  return (line) => {
    if (/^> ?/.test(line)) return line.replace(/^> ?/, '');
    return '> ' + line;
  };
}

function mdToggleBullet() {
  return (line) => {
    if (/^(\s*)[-*+] /.test(line)) return line.replace(/^(\s*)[-*+] /, '$1');
    const indentMatch = line.match(/^(\s*)(.*)$/);
    return (indentMatch[1] || '') + '- ' + (indentMatch[2] || '');
  };
}

function mdToggleOrdered() {
  return (line) => {
    if (/^(\s*)\d+[.)] /.test(line)) return line.replace(/^(\s*)\d+[.)] /, '$1');
    const indentMatch = line.match(/^(\s*)(.*)$/);
    return (indentMatch[1] || '') + '1. ' + (indentMatch[2] || '');
  };
}

function mdToggleTask() {
  return (line) => {
    if (/^(\s*)[-*+] \[[ xX]\] /.test(line)) {
      return line.replace(/^(\s*)[-*+] \[[ xX]\] /, '$1');
    }
    if (/^(\s*)[-*+] /.test(line)) {
      return line.replace(/^(\s*)[-*+] /, '$1- [ ] ');
    }
    const indentMatch = line.match(/^(\s*)(.*)$/);
    return (indentMatch[1] || '') + '- [ ] ' + (indentMatch[2] || '');
  };
}

async function mdInsertHr() {
  await mdFlushPreedit();
  snapshot();
  const [ls, le] = mdCurrentLineBounds(cursor);
  const before = committed.slice(0, ls);
  const mid = committed.slice(ls, le);
  const after = committed.slice(le);
  const pre = (mid.trim() === '') ? '' : mid + '\n';
  const insertion = pre + '---\n';
  committed = before + insertion + after;
  cursor = before.length + insertion.length;
  render();
}

async function mdInsertLink() {
  await mdFlushPreedit();
  const [s, e] = mdRange();
  snapshot();
  const sel = committed.slice(s, e);
  const label = sel || 'text';
  const snippet = `[${label}](url)`;
  committed = committed.slice(0, s) + snippet + committed.slice(e);
  // Place cursor where the user should type the URL.
  cursor = s + label.length + 3; // after "]("
  render();
}

async function mdInsertCodeBlock() {
  await mdFlushPreedit();
  snapshot();
  const [ls] = mdCurrentLineBounds(cursor);
  const block = '```\n\n```\n';
  committed = committed.slice(0, ls) + block + committed.slice(ls);
  cursor = ls + 4; // inside the fence, on the empty middle line
  render();
}

// ── 문단 메뉴 보조 함수 ──────────────────────────

// 제목 수준 올리기/낮추기: 현행 레벨에서 한 단계씩. 본문↔H1 토글도 포함.
function mdPromoteHeading() {
  return (line) => {
    const m = line.match(/^(#{1,6}) (.*)$/);
    if (!m) return line;  // 본문인 상태에서 "올리기" 는 무변화
    const level = m[1].length;
    if (level === 1) return m[2]; // H1 → 본문
    return '#'.repeat(level - 1) + ' ' + m[2];
  };
}
function mdDemoteHeading() {
  return (line) => {
    const m = line.match(/^(#{1,6}) (.*)$/);
    if (!m) return '# ' + line; // 본문 → H1
    const level = m[1].length;
    if (level === 6) return line; // H6 에서 멈춤
    return '#'.repeat(level + 1) + ' ' + m[2];
  };
}

async function mdInsertAtCurrentLine(text, caretOffsetInInsertion) {
  // 현재 라인이 공백 아닌 내용이 있으면 그 라인 뒤(새 줄)에 삽입하고,
  // 비어 있으면 그 자리에 바로 덮어 쓰기. caretOffsetInInsertion 은 삽입
  // 블록 기준 상대 오프셋(없으면 끝).
  await mdFlushPreedit();
  snapshot();
  const [ls, le] = mdCurrentLineBounds(cursor);
  const lineText = committed.slice(ls, le);
  const onEmpty = lineText.trim() === '';
  let insStart, insert;
  if (onEmpty) {
    insStart = ls;
    insert = text;
    committed = committed.slice(0, ls) + insert + committed.slice(le);
  } else {
    insStart = le;
    insert = '\n' + text;
    committed = committed.slice(0, le) + insert + committed.slice(le);
  }
  const off = (typeof caretOffsetInInsertion === 'number')
    ? caretOffsetInInsertion
    : insert.length;
  cursor = insStart + off;
  render();
}

async function mdInsertMathBlock() {
  // $$...$$ 블록. 가운데 빈 줄에 캐럿.
  await mdFlushPreedit();
  snapshot();
  const [ls, le] = mdCurrentLineBounds(cursor);
  const lineText = committed.slice(ls, le);
  const body = '$$\n\n$$\n';
  if (lineText.trim() === '') {
    committed = committed.slice(0, ls) + body + committed.slice(le);
    cursor = ls + 3; // "$$\n" 뒤 빈 줄
  } else {
    committed = committed.slice(0, le) + '\n' + body + committed.slice(le);
    cursor = le + 1 + 3;
  }
  render();
}

async function mdInsertCodeFence() {
  await mdInsertCodeBlock();
}

/* 표 크기 선택 모달 — 10×10 그리드에 마우스를 올려 행·열을 정하고,
   클릭·Enter 로 확정. Esc/바깥 클릭/취소 버튼은 null 반환.
   반환: { rows, cols } 또는 null. */
function pickTableSize() {
  return new Promise((resolve) => {
    const MAX = 10;
    const root = document.createElement('div');
    root.className = 'table-picker-modal';
    root.innerHTML = `
      <div class="table-picker-backdrop"></div>
      <div class="table-picker-panel" role="dialog" aria-modal="true" aria-label="표 크기 선택">
        <div class="table-picker-title">표 만들기</div>
        <div class="table-picker-hint">표 크기를 선택하세요 (최대 ${MAX} × ${MAX})</div>
        <div class="table-picker-grid" id="tp-grid"></div>
        <div class="table-picker-size" id="tp-size">3 × 3</div>
        <div class="table-picker-actions">
          <button type="button" class="btn btn-ghost" id="tp-cancel">취소</button>
          <button type="button" class="btn btn-primary" id="tp-ok">만들기</button>
        </div>
      </div>
    `;
    document.body.appendChild(root);
    const grid = root.querySelector('#tp-grid');
    const sizeEl = root.querySelector('#tp-size');
    let selR = 3, selC = 3;
    for (let r = 1; r <= MAX; r++) {
      for (let c = 1; c <= MAX; c++) {
        const cell = document.createElement('div');
        cell.className = 'tp-cell';
        cell.dataset.r = String(r);
        cell.dataset.c = String(c);
        grid.appendChild(cell);
      }
    }
    function highlight(r, c) {
      selR = r; selC = c;
      grid.querySelectorAll('.tp-cell').forEach((el) => {
        const cr = parseInt(el.dataset.r, 10);
        const cc = parseInt(el.dataset.c, 10);
        el.classList.toggle('active', cr <= r && cc <= c);
      });
      sizeEl.textContent = `${r} × ${c}`;
    }
    grid.addEventListener('mousemove', (ev) => {
      const cell = ev.target.closest && ev.target.closest('.tp-cell');
      if (cell) highlight(parseInt(cell.dataset.r, 10), parseInt(cell.dataset.c, 10));
    });
    // `click` 보다 `mousedown` 이 더 빨리 오고 드래그·포커스 변화와 덜
    // 충돌해서 양쪽을 다 바인딩한다. 이미 확정된 뒤에는 root.remove 로
    // 리스너가 같이 사라지므로 중복 호출 걱정 없음.
    const pickAt = (ev) => {
      const cell = ev.target.closest && ev.target.closest('.tp-cell');
      if (!cell) return;
      ev.preventDefault();
      finish({ rows: parseInt(cell.dataset.r, 10), cols: parseInt(cell.dataset.c, 10) });
    };
    grid.addEventListener('mousedown', pickAt);
    grid.addEventListener('click', pickAt);
    root.querySelector('#tp-ok').addEventListener('click', () => finish({ rows: selR, cols: selC }));
    root.querySelector('#tp-cancel').addEventListener('click', () => finish(null));
    root.querySelector('.table-picker-backdrop').addEventListener('click', () => finish(null));
    const onKey = (ev) => {
      if (ev.key === 'Escape') { ev.preventDefault(); finish(null); }
      else if (ev.key === 'Enter') { ev.preventDefault(); finish({ rows: selR, cols: selC }); }
      else if (ev.key.startsWith('Arrow')) {
        ev.preventDefault();
        let r = selR, c = selC;
        if (ev.key === 'ArrowUp') r = Math.max(1, r - 1);
        if (ev.key === 'ArrowDown') r = Math.min(MAX, r + 1);
        if (ev.key === 'ArrowLeft') c = Math.max(1, c - 1);
        if (ev.key === 'ArrowRight') c = Math.min(MAX, c + 1);
        highlight(r, c);
      }
    };
    window.addEventListener('keydown', onKey, true);
    function finish(v) {
      window.removeEventListener('keydown', onKey, true);
      root.remove();
      // 메뉴 선택 뒤 에디터로 포커스 복귀.
      try { editor.focus(); } catch {}
      resolve(v);
    }
    highlight(3, 3);
    // 초기 포커스: OK 버튼 (Enter 로 바로 확정 가능).
    root.querySelector('#tp-ok').focus();
  });
}

async function mdInsertTable(rows = 3, cols = 3) {
  await mdFlushPreedit();
  snapshot();
  const header = '| ' + Array.from({ length: cols }, (_, i) => `열 ${i + 1}`).join(' | ') + ' |';
  const sep = '|' + Array.from({ length: cols }, () => ' --- ').join('|') + '|';
  // body 셀은 처음부터 비워 둔다 — 셀에 미리 공백을 채워 두면, 사용자가
  // 입력한 문자열 양옆에 트레일링/리딩 공백이 남는 회귀가 생기기 때문.
  // 빈 셀의 caret 안착은 renderTableRow 의 0길이 text node 폴백이 책임진다.
  const body = Array.from({ length: Math.max(0, rows - 1) }, () => '|' + '|'.repeat(cols));
  const table = [header, sep, ...body].join('\n') + '\n';
  const [ls, le] = mdCurrentLineBounds(cursor);
  const lineText = committed.slice(ls, le);
  if (lineText.trim() === '') {
    committed = committed.slice(0, ls) + table + committed.slice(le);
    cursor = ls + 2; // 첫 셀 시작 근처
  } else {
    const insert = '\n' + table;
    committed = committed.slice(0, le) + insert + committed.slice(le);
    cursor = le + 1 + 2;
  }
  render();
}

/* ─────────── Editor context menu (필수 편집 명령만 한글로) ───────
   네이티브 WebView 컨텍스트 메뉴의 "Inspect Element", "Reload" 등
   개발자용 항목을 숨기고, 사용자가 기대하는 기본 편집 명령만 보여준다.
   표 셀 위에선 표 전용 메뉴가 우선이며, 그 외 영역에서 이 메뉴가 뜬다.*/
let editorCtxEl = null;
function ensureEditorCtxMenu() {
  if (editorCtxEl) return editorCtxEl;
  editorCtxEl = document.createElement('div');
  editorCtxEl.id = 'editor-ctxmenu';
  editorCtxEl.className = 'ctxmenu hidden';
  document.body.appendChild(editorCtxEl);
  return editorCtxEl;
}
function hideEditorCtxMenu() {
  if (editorCtxEl) editorCtxEl.classList.add('hidden');
}
function selectAllEditor() {
  const sel = window.getSelection();
  if (!sel) return;
  const range = document.createRange();
  range.selectNodeContents(editor);
  sel.removeAllRanges();
  sel.addRange(range);
}
function openEditorCtxMenu(ev) {
  const sel = window.getSelection();
  const hasSelection = !!(sel && !sel.isCollapsed
    && sel.toString().replace(/​/g, '').length > 0);
  const menu = ensureEditorCtxMenu();
  menu.innerHTML = '';
  const actions = [
    { label: '오려두기',  act: 'cut',    disabled: !hasSelection },
    { label: '복사하기',  act: 'copy',   disabled: !hasSelection },
    { label: '붙이기',    act: 'paste' },
    { sep: true },
    { label: '전체 선택', act: 'select-all' },
  ];
  for (const item of actions) {
    if (item.sep) {
      const s = document.createElement('div');
      s.className = 'ctx-sep';
      menu.appendChild(s);
      continue;
    }
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = item.label;
    if (item.disabled) b.disabled = true;
    b.addEventListener('click', async (e) => {
      e.stopPropagation();
      hideEditorCtxMenu();
      if (item.disabled) return;
      switch (item.act) {
        case 'cut':        await doCut(); break;
        case 'copy':       await doCopy(); break;
        case 'paste':      await doPaste(); break;
        case 'select-all': selectAllEditor(); break;
      }
    });
    menu.appendChild(b);
  }
  menu.classList.remove('hidden');
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const r = menu.getBoundingClientRect();
  const x = Math.min(ev.clientX, vw - r.width - 8);
  const y = Math.min(ev.clientY, vh - r.height - 8);
  menu.style.left = `${Math.max(8, x)}px`;
  menu.style.top = `${Math.max(8, y)}px`;
}
document.addEventListener('click', (ev) => {
  if (!editorCtxEl || editorCtxEl.classList.contains('hidden')) return;
  if (!ev.target.closest('#editor-ctxmenu')) hideEditorCtxMenu();
});
window.addEventListener('keydown', (ev) => {
  if (ev.key === 'Escape' && editorCtxEl && !editorCtxEl.classList.contains('hidden')) {
    ev.preventDefault();
    hideEditorCtxMenu();
  }
}, true);
window.addEventListener('blur', hideEditorCtxMenu);

/* ─────────── Table context menu (우클릭: 표/행/열 삭제) ─────────── */
let tableCtxEl = null;
function ensureTableCtxMenu() {
  if (tableCtxEl) return tableCtxEl;
  tableCtxEl = document.createElement('div');
  tableCtxEl.id = 'table-ctxmenu';
  tableCtxEl.className = 'ctxmenu hidden';
  document.body.appendChild(tableCtxEl);
  return tableCtxEl;
}
function hideTableCtxMenu() {
  if (tableCtxEl) tableCtxEl.classList.add('hidden');
}
editor.addEventListener('contextmenu', (ev) => {
  // 1) 표 셀 위에서의 우클릭 — 기존 표 전용 메뉴 (행/열/표 삭제).
  const td = markdownMode && ev.target.closest && ev.target.closest('td.md-cell');
  if (td) {
    const tr = td.closest('tr.md-line.md-table-row');
    const table = td.closest('table.md-table');
    if (tr && table) {
      ev.preventDefault();
      ev.stopPropagation();
      const rows = Array.from(table.querySelectorAll('tr.md-line.md-table-row'));
      const cells = Array.from(tr.querySelectorAll('td.md-cell'));
      const ctx = { tr, rows, tdIdx: cells.indexOf(td) };
      const menu = ensureTableCtxMenu();
      menu.innerHTML = '';
      const actions = [
        { label: '표 복사', act: 'copy-table' },
        { label: '표 붙여넣기', act: 'paste-table' },
        { sep: true },
        { label: '위에 행 추가', act: 'insert-row-above' },
        { label: '아래에 행 추가', act: 'insert-row-below' },
        { sep: true },
        { label: '왼쪽에 열 추가', act: 'insert-col-left' },
        { label: '오른쪽에 열 추가', act: 'insert-col-right' },
        { sep: true },
        { label: '행 삭제', act: 'delete-row' },
        { label: '열 삭제', act: 'delete-col' },
        { sep: true },
        { label: '표 삭제', act: 'delete-table', danger: true },
      ];
      for (const item of actions) {
        if (item.sep) {
          const s = document.createElement('div');
          s.className = 'ctx-sep';
          menu.appendChild(s);
          continue;
        }
        const b = document.createElement('button');
        b.type = 'button';
        b.textContent = item.label;
        if (item.danger) b.classList.add('danger');
        b.addEventListener('click', (e) => {
          e.stopPropagation();
          hideTableCtxMenu();
          runTableAction(item.act, ctx);
        });
        menu.appendChild(b);
      }
      menu.classList.remove('hidden');
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      const r = menu.getBoundingClientRect();
      const x = Math.min(ev.clientX, vw - r.width - 8);
      const y = Math.min(ev.clientY, vh - r.height - 8);
      menu.style.left = `${Math.max(8, x)}px`;
      menu.style.top = `${Math.max(8, y)}px`;
      return;
    }
  }
  // 2) 일반 영역 — 필수 편집 명령만 담은 커스텀 한글 메뉴.
  //    네이티브 WebView 컨텍스트 메뉴는 dev 모드에서 "Inspect Element"
  //    같은 개발자용 항목까지 포함되므로, preventDefault 로 차단한다.
  ev.preventDefault();
  ev.stopPropagation();
  openEditorCtxMenu(ev);
});
document.addEventListener('click', (ev) => {
  if (!tableCtxEl || tableCtxEl.classList.contains('hidden')) return;
  if (!ev.target.closest('#table-ctxmenu')) hideTableCtxMenu();
});
window.addEventListener('keydown', (ev) => {
  if (ev.key === 'Escape' && tableCtxEl && !tableCtxEl.classList.contains('hidden')) {
    ev.preventDefault();
    hideTableCtxMenu();
  }
}, true);
window.addEventListener('blur', hideTableCtxMenu);

async function runTableAction(act, ctx) {
  try {
    // 복사·붙여넣기는 텍스트 변경(snapshot 대상) 이 아니거나 doPaste 가
    // 자체적으로 snapshot 하므로 분기 진입 전 일괄 snapshot 을 걸지 않는다.
    if (act === 'copy-table') {
      await mdFlushPreedit();
      const first = ctx.rows[0];
      const last = ctx.rows[ctx.rows.length - 1];
      const s = sourceIdxAtLineStart(first);
      const e = sourceIdxAtLineStart(last) + lineSourceLength(last);
      const tableSource = committed.slice(s, e);
      const ok = await clipboardWrite(tableSource);
      logEvent(ok
        ? `표 복사됨 (${ctx.rows.length}행, ${truncate(tableSource, 40)})`
        : '표 복사 실패 (클립보드 접근 불가)');
      return;
    }
    if (act === 'paste-table') {
      // 클립보드 텍스트를 현재 cursor 위치에 그대로 붙여 넣는다 (Cmd+V 와
      // 동일한 doPaste 흐름). 클립보드에 마크다운 표 소스(`| ... |` 줄들) 가
      // 들어 있으면 render() 의 표 인식 로직이 자동으로 표로 다시 렌더한다.
      await doPaste();
      return;
    }
    await mdFlushPreedit();
    snapshot();
    if (act === 'delete-table') {
      const first = ctx.rows[0];
      const last = ctx.rows[ctx.rows.length - 1];
      const s = sourceIdxAtLineStart(first);
      const e = sourceIdxAtLineStart(last) + lineSourceLength(last);
      const trail = committed.charAt(e) === '\n' ? 1 : 0;
      committed = committed.slice(0, s) + committed.slice(e + trail);
      cursor = Math.min(cursor, s);
    } else if (act === 'delete-row') {
      if (ctx.tr.classList.contains('md-table-head') || ctx.tr.classList.contains('md-table-sep')) {
        logEvent('헤더/구분선 행은 삭제할 수 없습니다');
        return;
      }
      const s = sourceIdxAtLineStart(ctx.tr);
      const e = s + lineSourceLength(ctx.tr);
      const trail = committed.charAt(e) === '\n' ? 1 : 0;
      committed = committed.slice(0, s) + committed.slice(e + trail);
      cursor = Math.min(cursor, s);
    } else if (act === 'delete-col') {
      // 각 행에서 N번째 셀만 잘라 낸다. 뒤 행부터 처리해 인덱스가 흔들리지
      // 않게 한다.
      for (let i = ctx.rows.length - 1; i >= 0; i--) {
        const row = ctx.rows[i];
        const s = sourceIdxAtLineStart(row);
        const len = lineSourceLength(row);
        const lineText = committed.slice(s, s + len);
        const newLine = removeNthCellFromLine(lineText, ctx.tdIdx);
        committed = committed.slice(0, s) + newLine + committed.slice(s + len);
      }
      cursor = Math.min(cursor, committed.length);
    } else if (act === 'insert-row-above' || act === 'insert-row-below') {
      // 헤더/구분선 위에는 본문 형태의 행을 직접 끼워 넣을 수 없다 (표 구조
      // = "헤더 → 구분선 → 본문" 순서). 본문 행에서 우클릭 시에만 동작.
      const isHead = ctx.tr.classList.contains('md-table-head');
      const isSep = ctx.tr.classList.contains('md-table-sep');
      if (isHead || isSep) {
        logEvent('헤더/구분선 행에는 직접 행을 추가할 수 없습니다 — 본문 행에서 시도해 주세요');
        return;
      }
      const numCells = ctx.tr.querySelectorAll('td.md-cell').length;
      const newRow = '|' + '|'.repeat(numCells); // 빈 셀들 (트레일링 공백 없음)
      const s = sourceIdxAtLineStart(ctx.tr);
      const e = s + lineSourceLength(ctx.tr);
      if (act === 'insert-row-above') {
        committed = committed.slice(0, s) + newRow + '\n' + committed.slice(s);
        cursor = s + 1; // 새 행 첫 셀 안 (| 직후의 0길이 텍스트 노드)
      } else {
        committed = committed.slice(0, e) + '\n' + newRow + committed.slice(e);
        cursor = e + 1 + 1; // \n + | 직후
      }
    } else if (act === 'insert-col-left' || act === 'insert-col-right') {
      // 새 열은 표의 모든 행(헤더·구분선·본문) 동일 위치에 삽입한다.
      const targetIdx = act === 'insert-col-left' ? ctx.tdIdx : ctx.tdIdx + 1;
      // 헤더 라벨은 새로 만들어진 컬럼 번호 기준 — 기존 N개 → "열 N+1".
      const oldColCount = ctx.rows[0]
        ? ctx.rows[0].querySelectorAll('td.md-cell').length
        : 0;
      const headerLabel = `열 ${oldColCount + 1}`;
      // 뒤 행부터 처리 (각 행의 소스 길이가 바뀌어도 다른 행의 시작 인덱스
      // 가 함께 밀리도록).
      for (let i = ctx.rows.length - 1; i >= 0; i--) {
        const row = ctx.rows[i];
        const s = sourceIdxAtLineStart(row);
        const len = lineSourceLength(row);
        const lineText = committed.slice(s, s + len);
        const isHead = row.classList.contains('md-table-head');
        const isSep = row.classList.contains('md-table-sep');
        let cellContent;
        if (isHead) cellContent = ` ${headerLabel} `;
        else if (isSep) cellContent = ' --- ';
        else cellContent = ''; // 빈 본문 셀 (공백 없음)
        const newLine = insertCellInLine(lineText, targetIdx, cellContent);
        committed = committed.slice(0, s) + newLine + committed.slice(s + len);
      }
      cursor = Math.min(cursor, committed.length);
    }
    render();
  } catch (e) {
    logEvent(`표 편집 실패: ${e && e.message || e}`);
  }
}

/* 소스 라인에서 N번째 셀(0-indexed) 을 제거. 파이프 p[n] ~ p[n+1] 범위를
   잘라낸다(선행 파이프와 셀 내용 제거, 다음 파이프는 새 경계로 유지). */
function removeNthCellFromLine(line, n) {
  const leadMatch = line.match(/^(\s*)/);
  const leadWs = leadMatch[1];
  const rest = line.slice(leadWs.length);
  const pipePos = [];
  for (let i = 0; i < rest.length; i++) if (rest[i] === '|') pipePos.push(i);
  if (pipePos.length < 2 || n < 0 || n >= pipePos.length - 1) return line;
  const start = pipePos[n];
  const end = pipePos[n + 1];
  return leadWs + rest.slice(0, start) + rest.slice(end);
}

/* 표 라인 한 줄을 셀 단위로 분해. 표가 아니면 null. */
function splitTableLineCells(line) {
  const leadMatch = line.match(/^(\s*)/);
  const leadWs = leadMatch[1];
  const rest = line.slice(leadWs.length);
  const pipePos = [];
  for (let i = 0; i < rest.length; i++) if (rest[i] === '|') pipePos.push(i);
  if (pipePos.length < 2) return null;
  const cells = [];
  for (let i = 0; i < pipePos.length - 1; i++) {
    cells.push(rest.slice(pipePos[i] + 1, pipePos[i + 1]));
  }
  return { lead: leadWs, cells };
}

/* 분해한 셀 목록을 다시 표 라인으로 직렬화. */
function joinTableLineCells(lead, cells) {
  return lead + '|' + cells.join('|') + '|';
}

/* 표 라인의 N번째 위치(0-indexed) 에 새 셀을 삽입.
   N === 0 → 맨 앞, N === cells.length → 맨 끝. */
function insertCellInLine(line, at, content) {
  const split = splitTableLineCells(line);
  if (!split) return line;
  const idx = Math.max(0, Math.min(at, split.cells.length));
  split.cells.splice(idx, 0, content);
  return joinTableLineCells(split.lead, split.cells);
}

// GFM 알림 (admonition): "> [!NOTE]" 꼴.
async function mdInsertAdmonition(kind = 'NOTE') {
  const k = String(kind || 'NOTE').toUpperCase();
  const block = `> [!${k}]\n> \n`;
  await mdFlushPreedit();
  snapshot();
  const [ls, le] = mdCurrentLineBounds(cursor);
  const lineText = committed.slice(ls, le);
  const onEmpty = lineText.trim() === '';
  if (onEmpty) {
    committed = committed.slice(0, ls) + block + committed.slice(le);
    cursor = ls + `> [!${k}]\n> `.length; // 두 번째 줄의 "> " 뒤
  } else {
    const ins = '\n' + block;
    committed = committed.slice(0, le) + ins + committed.slice(le);
    cursor = le + 1 + `> [!${k}]\n> `.length;
  }
  render();
}

// 작업 상태 토글: 현재 라인이 작업 항목이면 체크/언체크/토글.
async function mdSetTaskState(state /* 'checked' | 'unchecked' | 'toggle' */) {
  await mdFlushPreedit();
  snapshot();
  const [ls, le] = mdCurrentLineBounds(cursor);
  const line = committed.slice(ls, le);
  const m = line.match(/^(\s*[-*+] )\[([ xX])\] (.*)$/);
  if (!m) { render(); return; }
  let next;
  if (state === 'checked') next = 'x';
  else if (state === 'unchecked') next = ' ';
  else next = (m[2].trim() === '') ? 'x' : ' '; // toggle
  const newLine = `${m[1]}[${next}] ${m[3]}`;
  committed = committed.slice(0, ls) + newLine + committed.slice(le);
  render();
}

// 들여쓰기 수준 증감. dir: +1 / -1 / 0 (완전히 평탄화).
async function mdAdjustIndent(dir) {
  await mdFlushPreedit();
  if (dir === 0) {
    snapshot();
    const [ls, le] = mdCurrentLineBounds(cursor);
    const line = committed.slice(ls, le);
    const stripped = line.replace(/^\s+/, '');
    const delta = stripped.length - line.length;
    committed = committed.slice(0, ls) + stripped + committed.slice(le);
    cursor = Math.max(ls, cursor + delta);
    render();
    return;
  }
  // +/- 는 기존 mdIndent 재사용.
  await mdIndent(dir > 0 ? 1 : -1);
}

// "앞에 본문 삽입" / "뒤에 본문 삽입" — 빈 단락 라인을 위/아래에 넣고
// 그 라인으로 커서를 옮긴다.
async function mdInsertParagraphBefore() {
  await mdFlushPreedit();
  snapshot();
  const [ls] = mdCurrentLineBounds(cursor);
  committed = committed.slice(0, ls) + '\n' + committed.slice(ls);
  cursor = ls;
  render();
}
async function mdInsertParagraphAfter() {
  await mdFlushPreedit();
  snapshot();
  const [, le] = mdCurrentLineBounds(cursor);
  const insertion = '\n\n';
  committed = committed.slice(0, le) + insertion + committed.slice(le);
  cursor = le + 1; // 새로 만들어진 빈 라인의 시작
  render();
}

// 각주: 캐럿 위치에 [^n] 을 넣고 문서 끝에 [^n]: 정의를 추가.
async function mdInsertFootnote() {
  await mdFlushPreedit();
  snapshot();
  // 기존 각주 id 최댓값 + 1 로 새 번호 부여.
  let maxN = 0;
  const re = /\[\^(\d+)\]/g;
  let m;
  while ((m = re.exec(committed))) {
    const n = parseInt(m[1], 10);
    if (!isNaN(n) && n > maxN) maxN = n;
  }
  const n = maxN + 1;
  const marker = `[^${n}]`;
  const def = `\n\n[^${n}]: `;
  const insertHere = committed.slice(0, cursor) + marker + committed.slice(cursor);
  committed = insertHere + (insertHere.endsWith('\n') ? '' : '') + def;
  cursor = committed.length; // 정의 라인 끝 — 사용자가 설명을 입력
  render();
}

async function mdInsertToc() {
  await mdInsertAtCurrentLine('[TOC]\n');
}

async function mdInsertYamlFrontMatter() {
  await mdFlushPreedit();
  snapshot();
  // 이미 앞에 --- 로 감싼 YAML 블록이 있으면 그대로 둔다.
  if (/^---\n[\s\S]*?\n---\n/.test(committed)) { render(); return; }
  const block = '---\ntitle: \n---\n\n';
  committed = block + committed;
  cursor = 'title: '.length + '---\n'.length; // title: 뒤
  render();
}

// 본문 메뉴의 "코드 도구 > 앞뒤 공백 제거": 현재 캐럿이 속한 코드블록의
// fenceIdx 를 찾아 기존 trimCodeBlock 에 위임한다. 코드블록 밖이면 no-op.
async function mdTrimCodeBlockAtCaret() {
  await mdFlushPreedit();
  const lines = committed.split('\n');
  // 소스 인덱스 → 라인 인덱스
  let acc = 0;
  let caretLineIdx = 0;
  for (let i = 0; i < lines.length; i++) {
    if (cursor <= acc + lines[i].length) { caretLineIdx = i; break; }
    acc += lines[i].length + 1;
    caretLineIdx = i + 1;
  }
  // 해당 라인이 펜스 사이에 있는지, 그리고 몇 번째 블록인지 찾는다.
  let fenceIdx = -1;
  let inFence = false;
  let belongsTo = -1;
  for (let i = 0; i < lines.length; i++) {
    const isFence = /^\s*```/.test(lines[i]);
    if (isFence && !inFence) {
      inFence = true;
      fenceIdx += 1;
      if (i === caretLineIdx) belongsTo = fenceIdx;
    } else if (isFence && inFence) {
      inFence = false;
      if (i === caretLineIdx) belongsTo = fenceIdx;
    } else if (inFence && i === caretLineIdx) {
      belongsTo = fenceIdx;
    }
  }
  if (belongsTo < 0) return;
  trimCodeBlock(belongsTo);
}

async function handleMdShortcut(ev) {
  const code = ev.code;
  const shift = ev.shiftKey;
  const alt = ev.altKey;
  // (shift, alt) 조합에 따라 명확히 분기한다. alt 체크를 안 하면 ⌥⌘B 가
  // ⌘B(볼드) 에 흡수돼 버려서 본문 메뉴의 accelerator 가 발화할 기회가
  // 사라진다(JS 가 preventDefault 한 뒤엔 OS 메뉴까지 이벤트가 안 감).
  if (!shift && !alt) {
    // Cmd/Ctrl + Key
    switch (code) {
      case 'KeyB': await mdWrap('**', '**'); return true;
      case 'KeyI': await mdWrap('*', '*'); return true;
      case 'KeyE': await mdWrap('`', '`'); return true;
      case 'KeyK': await mdInsertLink(); return true;
      case 'KeyL': await mdReplaceLinePrefix(mdToggleBullet()); return true;
      case 'Digit0': await mdReplaceLinePrefix(mdToggleHeading(0)); return true;
      case 'Digit1': await mdReplaceLinePrefix(mdToggleHeading(1)); return true;
      case 'Digit2': await mdReplaceLinePrefix(mdToggleHeading(2)); return true;
      case 'Digit3': await mdReplaceLinePrefix(mdToggleHeading(3)); return true;
      case 'Digit4': await mdReplaceLinePrefix(mdToggleHeading(4)); return true;
      case 'Digit5': await mdReplaceLinePrefix(mdToggleHeading(5)); return true;
      case 'Digit6': await mdReplaceLinePrefix(mdToggleHeading(6)); return true;
      // 본문 메뉴: 제목 수준 올리기 ⌘= / 낮추기 ⌘-
      case 'Equal': await mdReplaceLinePrefix(mdPromoteHeading()); return true;
      case 'Minus': await mdReplaceLinePrefix(mdDemoteHeading()); return true;
    }
  } else if (shift && !alt) {
    // Shift + Cmd/Ctrl + Key (기존 단축키 유지)
    switch (code) {
      case 'KeyX': await mdWrap('~~', '~~'); return true;
      case 'KeyQ': await mdReplaceLinePrefix(mdToggleQuote()); return true;
      case 'KeyK': await mdInsertCodeBlock(); return true;
      case 'KeyO': await mdReplaceLinePrefix(mdToggleOrdered()); return true;
      case 'KeyT': await mdReplaceLinePrefix(mdToggleTask()); return true;
      case 'Minus': await mdInsertHr(); return true;
    }
  } else if (!shift && alt) {
    // Alt + Cmd/Ctrl + Key — 문단 메뉴 단축키 chord.
    switch (code) {
      case 'KeyB': await mdInsertMathBlock(); return true;
      case 'KeyC': await mdInsertCodeFence(); return true;
      case 'KeyQ': await mdReplaceLinePrefix(mdToggleQuote()); return true;
      case 'KeyO': await mdReplaceLinePrefix(mdToggleOrdered()); return true;
      case 'KeyU': await mdReplaceLinePrefix(mdToggleBullet()); return true;
      case 'KeyX': await mdReplaceLinePrefix(mdToggleTask()); return true;
      case 'KeyL': await mdInsertLink(); return true;
      case 'KeyR': await mdInsertFootnote(); return true;
      case 'Minus': await mdInsertHr(); return true;
    }
  }
  return false;
}

async function mdListContinue() {
  await mdFlushPreedit();
  const [ls, le] = mdCurrentLineBounds(cursor);
  const line = committed.slice(ls, le);
  // Task, bullet, or ordered.
  let m = line.match(/^(\s*)([-*+]) \[[ xX]\] (.*)$/);
  let kind = null;
  let indent, marker, content;
  if (m) { kind = 'task'; indent = m[1]; marker = m[2]; content = m[3]; }
  else {
    m = line.match(/^(\s*)([-*+]) (.*)$/);
    if (m) { kind = 'bullet'; indent = m[1]; marker = m[2]; content = m[3]; }
    else {
      m = line.match(/^(\s*)(\d+)([.)]) (.*)$/);
      if (m) { kind = 'ol'; indent = m[1]; marker = m[2]; content = m[4]; }
    }
  }
  if (!kind) return false;
  snapshot();
  // Empty item → exit the list.
  if (content.trim() === '') {
    committed = committed.slice(0, ls) + committed.slice(le);
    cursor = ls;
    render();
    return true;
  }
  let prefix;
  if (kind === 'task') prefix = `${indent}${marker} [ ] `;
  else if (kind === 'bullet') prefix = `${indent}${marker} `;
  else {
    const next = parseInt(marker, 10) + 1;
    const sep = m[3];
    prefix = `${indent}${next}${sep} `;
  }
  const insertion = '\n' + prefix;
  committed = committed.slice(0, cursor) + insertion + committed.slice(cursor);
  cursor += insertion.length;
  render();
  return true;
}

async function mdIndent(dir) {
  const [ls, le] = mdCurrentLineBounds(cursor);
  const line = committed.slice(ls, le);
  // Only operate on list lines.
  if (!/^\s*(?:[-*+]|\d+[.)]) /.test(line)) return false;
  snapshot();
  let newLine;
  if (dir > 0) {
    newLine = '  ' + line;
    cursor += 2;
  } else {
    if (!line.startsWith('  ')) return false;
    newLine = line.slice(2);
    cursor = Math.max(ls, cursor - 2);
  }
  committed = committed.slice(0, ls) + newLine + committed.slice(le);
  render();
  return true;
}

/* Toggle a task-list checkbox by flipping the `x`/` ` char in source. */
function toggleTaskLine(lineEl) {
  let acc = 0;
  let found = false;
  for (const el of editor.querySelectorAll('.md-line')) {
    if (el === lineEl) { found = true; break; }
    acc += lineSourceLength(el) + 1;
  }
  if (!found) return;
  const lineStart = acc;
  const lineLen = lineSourceLength(lineEl);
  const lineText = committed.slice(lineStart, lineStart + lineLen);
  const m = lineText.match(/^(\s*)([-*+]) \[([ xX])\] /);
  if (!m) return;
  const charIdx = lineStart + m[1].length + 3; // after indent + "- ["
  const newChar = m[3] === ' ' ? 'x' : ' ';
  snapshot();
  committed = committed.slice(0, charIdx) + newChar + committed.slice(charIdx + 1);
  // Don't move cursor — leave it wherever it was.
  render();
}

/* ═══════════════════════ Cursor sync from selection ═══════════════════════ */
editor.addEventListener('mousedown', (ev) => {
  // Mouse anywhere ends image selection — either the user clicked the
  // image (reselected below), landed on text (moved caret away), or
  // clicked whitespace (deselect, let caret settle naturally).
  if (selectedImage && ev.target !== selectedImage) deselectImage();
  // 클릭으로 선택 위치가 이동할 때 preedit 가 남아 있으면 먼저 커밋해
  // DOM 을 재생성한 다음 계산해야 하지만, 플러시 결과가 돌아오면 DOM 이
  // 교체돼 click 으로 받은 browser selection 이 detached 상태가 된다.
  // 대신 **클릭 좌표** 를 기억해 두었다가 render 직후 caretRange­FromPoint
  // 로 새 DOM 상에서 동일 지점을 다시 찾아 캐럿을 앉힌다. 좌표가 없으면
  // (비-primary 버튼 등) 기존 로직을 쓴다.
  if (preedit && ev.button === 0) {
    const px = ev.clientX;
    const py = ev.clientY;
    invoke('flush').catch(() => null).then((resp) => {
      if (resp && resp.commit) {
        committed = committed.slice(0, cursor) + resp.commit + committed.slice(cursor);
        cursor += resp.commit.length;
      }
      preedit = '';
      render();
      const hit = (document.caretRangeFromPoint
        ? document.caretRangeFromPoint(px, py)
        : null);
      if (hit && editor.contains(hit.startContainer)) {
        const idx = domToSourceIdx(hit.startContainer, hit.startOffset);
        if (typeof idx === 'number') {
          cursor = idx;
          placeCaretAtSourceIdx(cursor);
        }
      }
    });
  }
});
editor.addEventListener('click', (ev) => {
  if (isOsImeMode()) return;
  // Intercept task-checkbox clicks before cursor sync so the click
  // doesn't try to land a caret inside the SVG.
  const chk = ev.target.closest && ev.target.closest('.md-task-check');
  if (chk) {
    ev.preventDefault();
    ev.stopPropagation();
    const line = chk.closest('.md-line.md-task');
    if (line) toggleTaskLine(line);
    return;
  }
  // Markdown link navigation — Cmd/Ctrl+Click opens externally.
  const anchor = ev.target.closest && ev.target.closest('.md-link a, .md-autolink a');
  if (anchor && (ev.metaKey || ev.ctrlKey)) {
    ev.preventDefault();
    ev.stopPropagation();
    const href = anchor.getAttribute('href');
    if (href) invoke('open_url', { url: href }).catch((e) => logEvent(`링크 열기 실패: ${e}`));
    return;
  }
  // Wiki-link — Cmd/Ctrl+Click 으로 내부 문서 열기/만들기.
  const wiki = ev.target.closest && ev.target.closest('.md-wikilink');
  if (wiki && (ev.metaKey || ev.ctrlKey)) {
    handleWikilinkClick(ev);
    return;
  }
  syncCursorFromSelection();
  // 표 빈 셀을 클릭하면 브라우저가 caret 을 td-element 의 좌측 가장자리
  // (padding 밖) 에 두는 케이스가 있어, syncCursor 로 source 위치는 맞아도
  // 시각적 caret 은 셀 바깥에 그대로 머문다. cursor 기준으로 DOM caret 을
  // 재배치해 가장 가까운 텍스트 노드(빈 셀이라면 0길이 nudge) 위로 옮긴다.
  if (markdownMode) {
    const td = ev.target && ev.target.closest && ev.target.closest('td.md-cell');
    if (td) placeCaretAtSourceIdx(cursor);
  }
  updateCaretLineFromSelection();
  ensureCaretVisible();
  if (markdownMode) updateMdToolbar();
});
editor.addEventListener('keyup', (ev) => {
  if (isOsImeMode()) return;
  if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End',
       'PageUp', 'PageDown'].includes(ev.code)) {
    syncCursorFromSelection();
    updateCaretLineFromSelection();
    ensureCaretVisible();
    if (markdownMode) updateMdToolbar();
  }
});
document.addEventListener('selectionchange', () => {
  if (markdownMode && !isOsImeMode()) {
    updateCaretLineFromSelection();
    updateMdToolbar();
  }
});

function syncCursorFromSelection() {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return;
  const r = sel.getRangeAt(0);
  if (!editor.contains(r.startContainer)) return;
  // If composition is pending and user moves the caret, flush first so
  // the new cursor is the insertion point for follow-up keystrokes.
  if (preedit) {
    invoke('flush').catch(() => null).then((resp) => {
      if (resp && resp.commit) {
        committed = committed.slice(0, cursor) + resp.commit + committed.slice(cursor);
        cursor += resp.commit.length;
      }
      preedit = '';
      // Re-anchor after the flush re-render consumed the selection.
      const newIdx = domToSourceIdx(r.startContainer, r.startOffset);
      if (typeof newIdx === 'number') cursor = newIdx;
      render();
    });
    return;
  }
  const idx = domToSourceIdx(r.startContainer, r.startOffset);
  if (typeof idx === 'number') cursor = idx;
}

/* ═══════════════════════ Floating format toolbar ═══════════════════════ */
function updateMdToolbar() {
  if (!mdToolbar || !markdownMode) {
    if (mdToolbar) mdToolbar.classList.add('hidden');
    return;
  }
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || !sel.rangeCount || !editor.contains(sel.anchorNode)) {
    mdToolbar.classList.add('hidden');
    return;
  }
  const rect = sel.getRangeAt(0).getBoundingClientRect();
  if (!rect.width && !rect.height) {
    mdToolbar.classList.add('hidden');
    return;
  }
  mdToolbar.classList.remove('hidden');
  const tb = mdToolbar.getBoundingClientRect();
  const gap = 8;
  let top = rect.top - tb.height - gap;
  if (top < 8) top = rect.bottom + gap;
  let left = rect.left + rect.width / 2 - tb.width / 2;
  left = Math.max(8, Math.min(left, window.innerWidth - tb.width - 8));
  mdToolbar.style.top = `${top}px`;
  mdToolbar.style.left = `${left}px`;
}

if (mdToolbar) {
  mdToolbar.addEventListener('mousedown', (ev) => ev.preventDefault()); // keep selection
  mdToolbar.addEventListener('click', async (ev) => {
    const btn = ev.target.closest('button[data-md]');
    if (!btn) return;
    ev.preventDefault();
    const act = btn.dataset.md;
    switch (act) {
      case 'bold': await mdWrap('**', '**'); break;
      case 'italic': await mdWrap('*', '*'); break;
      case 'strike': await mdWrap('~~', '~~'); break;
      case 'code': await mdWrap('`', '`'); break;
      case 'h1': await mdReplaceLinePrefix(mdToggleHeading(1)); break;
      case 'h2': await mdReplaceLinePrefix(mdToggleHeading(2)); break;
      case 'h3': await mdReplaceLinePrefix(mdToggleHeading(3)); break;
      case 'ul': await mdReplaceLinePrefix(mdToggleBullet()); break;
      case 'ol': await mdReplaceLinePrefix(mdToggleOrdered()); break;
      case 'task': await mdReplaceLinePrefix(mdToggleTask()); break;
      case 'quote': await mdReplaceLinePrefix(mdToggleQuote()); break;
      case 'link': await mdInsertLink(); break;
      case 'hr': await mdInsertHr(); break;
    }
    mdToolbar.classList.add('hidden');
    editor.focus();
  });
}

/* ═══════════════════════ Markdown toggle wiring ═══════════════════════ */
if (mdToggle) {
  mdToggle.checked = markdownMode;
  mdToggle.addEventListener('change', async () => {
    markdownMode = mdToggle.checked;
    if (!markdownMode && mdToolbar) mdToolbar.classList.add('hidden');
    cursor = Math.max(0, Math.min(cursor, committed.length));
    try { localStorage.setItem(MD_KEY, markdownMode ? '1' : '0'); } catch (_) {}
    render();
  });
}

// 보기 → 소스코드 보기: WYSIWYG(markdownMode=true)과 원문 보기를
// 오가는 토글. 설정 모달의 md-toggle 체크박스와 같은 상태를 공유한다.
function toggleSourceView() {
  markdownMode = !markdownMode;
  if (!markdownMode && mdToolbar) mdToolbar.classList.add('hidden');
  cursor = Math.max(0, Math.min(cursor, committed.length));
  try { localStorage.setItem(MD_KEY, markdownMode ? '1' : '0'); } catch (_) {}
  if (mdToggle) mdToggle.checked = markdownMode;
  syncMenuCheck('toggle-source', markdownMode);
  render();
}

/* ─────────── View menu — native menu check sync ─────────── */
// JS 상태를 단일 진실 소스로 쓰고, 체크 메뉴의 실제 표시는 `set_menu_check`
// 로 매 토글마다 덮어쓴다. macOS 의 auto-toggle 이 있든 없든 마지막에 명시
// 값으로 set_checked 를 호출하기 때문에 결과가 항상 일관된다.
async function syncMenuCheck(id, on) {
  try { await invoke('set_menu_check', { id, on: !!on }); } catch (_) {}
}

/* ─────────── View menu — sidebar view (개요 보기 토글) ─────────── */
// 사이드바는 기본적으로 파일 트리를 보여 준다. 메뉴 '사이드바 ― 개요 보기'
// 를 체크하면 개요 뷰로 전환, 체크를 해제하면 다시 파일 트리로 돌아간다.
// 즉 체크 상태 = outline, 해제 상태 = files.
const SIDEBAR_VIEW_KEY = 'leaf-ime:sidebar-view';
function currentSidebarView() {
  const pane = document.getElementById('files-pane');
  const v = pane && pane.dataset.view;
  return v === 'outline' ? 'outline' : 'files';
}
function syncSidebarMenus() {
  syncMenuCheck('sidebar-outline', currentSidebarView() === 'outline');
}
function switchSidebarView(view) {
  const pane = document.getElementById('files-pane');
  if (!pane) return;
  const next = view === 'outline' ? 'outline' : 'files';
  pane.dataset.view = next;
  try { localStorage.setItem(SIDEBAR_VIEW_KEY, next); } catch {}
  renderSidebarView(next);
  syncSidebarMenus();
}
function toggleOutlineView() {
  switchSidebarView(currentSidebarView() === 'outline' ? 'files' : 'outline');
}
// Initial sync — 저장된 값이 있으면 복원, 없으면 기본값 'files'.
(function () {
  const savedView = (() => {
    try { return localStorage.getItem(SIDEBAR_VIEW_KEY) || 'files'; }
    catch { return 'files'; }
  })();
  const pane = document.getElementById('files-pane');
  if (pane) {
    pane.dataset.view = savedView === 'outline' ? 'outline' : 'files';
    renderSidebarView(pane.dataset.view);
  }
  syncSidebarMenus();
})();

/* ─────────── Sidebar view rendering (파일 트리 · 개요 · 문서) ─────────── */
// 파일 트리는 기존 DOM 을 그대로 사용한다. 개요/문서 뷰는 파일 트리 DOM 을
// 그대로 둔 채 overlay 로 덮어 전환하는 방식이라, 뷰를 닫으면 overlay 만
// 제거하면 바로 파일 트리로 돌아간다.
function renderSidebarView(view) {
  const pane = document.getElementById('files-pane');
  if (!pane) return;
  let overlay = pane.querySelector('.sidebar-view-overlay');
  if (view === 'files') {
    if (overlay) overlay.remove();
    return;
  }
  if (!overlay) {
    overlay = document.createElement('div');
    overlay.className = 'sidebar-view-overlay';
    pane.appendChild(overlay);
  }
  if (view === 'outline') renderSidebarOutline(overlay);
}
function computeOutline() {
  const ed = document.getElementById('editor');
  const text = (ed ? ed.innerText : '') || '';
  const out = [];
  text.split('\n').forEach((line) => {
    const m = line.match(/^(#{1,6})\s+(.+)$/);
    if (m) out.push({ level: m[1].length, text: m[2].trim() });
  });
  return out;
}
function renderSidebarOutline(overlay) {
  const items = computeOutline();
  if (items.length === 0) {
    overlay.innerHTML =
      `<div class="sb-view-head">개요</div>` +
      `<div class="sb-view-empty">제목이 없습니다.</div>`;
    return;
  }
  const rows = items.map((it, idx) =>
    `<div class="sb-view-row sb-outline-row" data-idx="${idx}" ` +
      `style="padding-left:${8 + (it.level - 1) * 12}px" ` +
      `title="제목 ${it.level}"><span class="sb-outline-lvl">H${it.level}</span>${escapeHtml(it.text)}</div>`
  ).join('');
  overlay.innerHTML = `<div class="sb-view-head">개요</div><div class="sb-view-body">${rows}</div>`;
  overlay.querySelectorAll('.sb-outline-row').forEach((row) => {
    row.addEventListener('click', () => {
      const idx = parseInt(row.dataset.idx || '0', 10);
      scrollToOutlineHeading(idx);
    });
  });
}
function escapeHtml(s) {
  return String(s)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}
// Scroll the editor so the n-th markdown heading (by document order) is
// near the top of the viewport.
function scrollToOutlineHeading(n) {
  const ed = document.getElementById('editor');
  if (!ed) return;
  const headings = ed.querySelectorAll('h1, h2, h3, h4, h5, h6');
  const target = headings[n];
  if (target && target.scrollIntoView) {
    target.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }
}


/* ─────────── Status bar — 단어수 · 현재 줄 표시 ─────────── */
// 에디터 우측 하단 상태표시줄에 항상 표시되는 인디케이터. 예: "Ln 42/118 · 320 단어".
// input 이벤트 + selection 변경 양쪽에서 갱신한다.
function computeWordCount() {
  const ed = document.getElementById('editor');
  const text = (ed ? ed.innerText : '') || '';
  return {
    chars: text.length,
    words: (text.trim().match(/\S+/g) || []).length,
    lines: text.split('\n').length,
  };
}
function currentLineNumber() {
  // committed + preedit 상의 cursor 위치를 라인 인덱스로 변환. 실패 시
  // 셀렉션 기반 근사.
  const src = typeof committed === 'string' ? committed : '';
  const pos = typeof cursor === 'number' ? cursor : 0;
  if (src) return src.slice(0, pos).split('\n').length;
  // fallback: DOM selection
  const sel = window.getSelection();
  const ed = document.getElementById('editor');
  if (!ed || !sel || sel.rangeCount === 0) return 1;
  const r = sel.getRangeAt(0);
  if (!ed.contains(r.startContainer)) return 1;
  const pre = document.createRange();
  pre.selectNodeContents(ed);
  pre.setEnd(r.startContainer, r.startOffset);
  return pre.toString().split('\n').length;
}
function ensureStatusStatsEl() {
  let el = document.getElementById('status-stats');
  if (el) return el;
  const right = document.querySelector('#status-bar .status-right');
  if (!right) return null;
  el = document.createElement('span');
  el.id = 'status-stats';
  el.className = 'status-item status-stats';
  // 오른쪽 영역에서 언어 토글 좌측에 삽입 (언어 토글은 끝에 두고 싶다).
  right.insertBefore(el, right.firstChild);
  return el;
}

function updateStatusStats() {
  const el = ensureStatusStatsEl();
  if (!el) return;
  const s = computeWordCount();
  const ln = currentLineNumber();
  el.textContent = `Ln ${ln}/${s.lines} · ${s.words} 단어 · ${s.chars}자`;
}
(function () {
  const ed = document.getElementById('editor');
  if (!ed) return;
  ed.addEventListener('input', () => {
    updateStatusStats();
    const pane = document.getElementById('files-pane');
    if (pane && pane.dataset.view === 'outline') renderSidebarView('outline');
  });
  document.addEventListener('selectionchange', updateStatusStats);
  setTimeout(updateStatusStats, 0);
})();

/* ─────────── View menu — zoom ─────────── */
// `--ui-scale` 은 UI chrome 폰트에만 적용되고 본문에는 걸리지 않으므로,
// 브라우저의 `zoom` CSS 속성(또는 transform scale 대안)으로 본문까지
// 한 번에 키운다. WebKit/Chromium 은 zoom 을 지원한다.
const ZOOM_ALLOW_KEY = 'leaf-ime:zoom-allow';
const ZOOM_LEVEL_KEY = 'leaf-ime:zoom-level';
let zoomAllow = (localStorage.getItem(ZOOM_ALLOW_KEY) ?? '1') === '1';
let zoomLevel = parseFloat(localStorage.getItem(ZOOM_LEVEL_KEY) || '1') || 1;
function applyZoom(showBadge = true) {
  const v = zoomAllow ? zoomLevel : 1;
  // UI chrome 폰트 스케일 — 사이드바/상태표시줄 등 칩 요소의 비율 유지.
  document.documentElement.style.setProperty('--ui-scale', String(v));
  // 본문 포함 전체 화면 확대·축소. WebKit(WKWebView) 과 Chromium 모두
  // 비표준 CSS `zoom` 속성을 지원한다. 네이티브 webview.set_zoom 경로는
  // Tauri v2 에서 권한·빌드 옵션에 따라 침묵 실패하는 경우가 있어,
  // 더 단순하고 확정적인 body.style.zoom 을 단일 진실로 쓴다.
  try { document.body.style.zoom = String(v); } catch { /* noop */ }
  syncMenuCheck('toggle-zoom-allow', zoomAllow);
  if (showBadge) showZoomBadge();
}

/* 현재 배율을 잠시 표시해 주는 상단 플로팅 레이어. 사용자가 현재 몇 %
   로 확대/축소됐는지 한눈에 볼 수 있도록 Cmd+휠·메뉴·단축키 등 배율이
   바뀌는 모든 경로에서 호출된다. 버튼으로도 즉시 조작 가능. */
let zoomBadgeHideTimer = 0;
function ensureZoomBadge() {
  let el = document.getElementById('zoom-badge');
  if (el) return el;
  el = document.createElement('div');
  el.id = 'zoom-badge';
  el.className = 'zoom-badge hidden';
  el.innerHTML = `
    <span class="zb-pct" id="zb-pct">100%</span>
    <button class="zb-btn" id="zb-minus" title="축소" aria-label="축소">−</button>
    <button class="zb-btn" id="zb-plus" title="확대" aria-label="확대">+</button>
    <span class="zb-sep" aria-hidden="true"></span>
    <button class="zb-reset" id="zb-reset" title="실제 크기">Reset</button>
  `;
  document.body.appendChild(el);
  el.addEventListener('mouseenter', () => {
    if (zoomBadgeHideTimer) { clearTimeout(zoomBadgeHideTimer); zoomBadgeHideTimer = 0; }
  });
  el.addEventListener('mouseleave', () => scheduleZoomBadgeHide(1500));
  document.getElementById('zb-minus').addEventListener('click', () => { zoomOut(); });
  document.getElementById('zb-plus').addEventListener('click', () => { zoomIn(); });
  document.getElementById('zb-reset').addEventListener('click', () => { zoomReset(); });
  return el;
}
function scheduleZoomBadgeHide(delay) {
  if (zoomBadgeHideTimer) clearTimeout(zoomBadgeHideTimer);
  zoomBadgeHideTimer = setTimeout(() => {
    const el = document.getElementById('zoom-badge');
    if (el) el.classList.add('hidden');
    zoomBadgeHideTimer = 0;
  }, delay);
}
function showZoomBadge() {
  const el = ensureZoomBadge();
  const pctEl = document.getElementById('zb-pct');
  if (pctEl) pctEl.textContent = `${Math.round(zoomLevel * 100)}%`;
  el.classList.remove('hidden');
  scheduleZoomBadgeHide(1800);
}
function toggleZoomAllow() {
  zoomAllow = !zoomAllow;
  try { localStorage.setItem(ZOOM_ALLOW_KEY, zoomAllow ? '1' : '0'); } catch {}
  applyZoom();
}
function clampZoom(v) { return Math.max(0.5, Math.min(2.5, Math.round(v * 100) / 100)); }
function zoomReset() {
  if (!zoomAllow) return;
  zoomLevel = 1;
  try { localStorage.setItem(ZOOM_LEVEL_KEY, '1'); } catch {}
  applyZoom();
  logEvent(`배율 100%`);
}
function zoomIn() {
  if (!zoomAllow) return;
  zoomLevel = clampZoom(zoomLevel + 0.1);
  try { localStorage.setItem(ZOOM_LEVEL_KEY, String(zoomLevel)); } catch {}
  applyZoom();
  logEvent(`배율 ${Math.round(zoomLevel * 100)}%`);
}
function zoomOut() {
  if (!zoomAllow) return;
  zoomLevel = clampZoom(zoomLevel - 0.1);
  try { localStorage.setItem(ZOOM_LEVEL_KEY, String(zoomLevel)); } catch {}
  applyZoom();
  logEvent(`배율 ${Math.round(zoomLevel * 100)}%`);
}
// 초기화 경로에서는 배지를 띄우지 않는다 — 사용자가 배율을 바꾼 적 없는
// 시점에서 "100%" 배지가 번쩍이는 건 노이즈라서.
applyZoom(false);

// Cmd(macOS) / Ctrl(Win,Linux) + 마우스 휠(또는 macOS 트랙패드 핀치 —
// 브라우저에서 `ctrlKey + wheel` 로 전달됨) 로 화면 확대·축소.
// • 캡처 단계에서 최상위로 잡고, 모든 타깃(window/document/editor)에 중복
//   부착해 자식 요소의 wheel 핸들러가 이벤트를 삼켜도 안정적으로 동작.
// • preventDefault + stopPropagation 으로 뒤이어 일반 스크롤·확대 기본
//   동작이 이어지는 걸 차단.
// • 60ms 소프트 스로틀로 빠른 휠 폭주 방지.
let lastWheelZoomAt = 0;
function handleWheelZoom(ev) {
  const mod = ev.ctrlKey || ev.metaKey;
  if (!mod) return;
  if (!zoomAllow) {
    // 확대 허용이 꺼져 있어도 Cmd+휠은 시스템 기본 동작(브라우저 줌 등)을
    // 쓰지 않도록 차단하고 조용히 무시한다.
    ev.preventDefault();
    return;
  }
  ev.preventDefault();
  ev.stopPropagation();
  const now = Date.now();
  if (now - lastWheelZoomAt < 60) return;
  lastWheelZoomAt = now;
  if (ev.deltaY < 0) zoomIn();
  else if (ev.deltaY > 0) zoomOut();
}
const wheelOpts = { passive: false, capture: true };
window.addEventListener('wheel', handleWheelZoom, wheelOpts);
document.addEventListener('wheel', handleWheelZoom, wheelOpts);
// Editor 엘리먼트도 직접 대상으로 — WKWebView 에서 컨텐트edit 요소 안쪽
// 으로는 캡처가 전파 안 되는 경우가 있어 명시적으로 걸어 둔다.
(function bindEditorWheel() {
  const ed = document.getElementById('editor');
  if (!ed) return;
  ed.addEventListener('wheel', handleWheelZoom, wheelOpts);
})();

/* ─────────── View menu — always-on-top / fullscreen ─────────── */
const ALWAYS_ON_TOP_KEY = 'leaf-ime:always-on-top';
let alwaysOnTop = localStorage.getItem(ALWAYS_ON_TOP_KEY) === '1';
async function toggleAlwaysOnTop() {
  // JS 상태를 진실로 쓰고, 창·메뉴 체크를 둘 다 명시적으로 덮어쓴다.
  // 항상 위에 OFF: 다른 창을 선택하면 그 창이 앞으로 와야 하고, ON: 우리
  // 창이 모든 창 위에 떠 있어야 한다.
  alwaysOnTop = !alwaysOnTop;
  try { localStorage.setItem(ALWAYS_ON_TOP_KEY, alwaysOnTop ? '1' : '0'); } catch {}
  try { await invoke('set_always_on_top', { on: alwaysOnTop }); }
  catch (e) { logEvent(`항상 위에 실패: ${e}`); }
  syncMenuCheck('toggle-always-on-top', alwaysOnTop);
  logEvent(`항상 위에 ${alwaysOnTop ? '켬' : '끔'}`);
}
(async () => {
  // 앱 시작 시 저장된 상태를 창과 메뉴에 모두 반영한다.
  try { await invoke('set_always_on_top', { on: alwaysOnTop }); } catch {}
  syncMenuCheck('toggle-always-on-top', alwaysOnTop);
})();
async function toggleFullscreen() {
  // 현재 창 상태를 Rust 에서 읽어 그 반대로 전환한다. 네이티브 녹색 버튼
  // 경로는 아래 resize 훅이 뒤이어 메뉴 체크를 맞춰 준다.
  try {
    const now = await invoke('is_window_fullscreen');
    const next = !now;
    await invoke('set_window_fullscreen', { on: next });
    syncMenuCheck('toggle-fullscreen', next);
  } catch (e) { logEvent(`전체 화면 실패: ${e}`); }
}

// 시작 시 한 번만 모든 상태 항목의 체크를 맞춰 둔다. 위의 apply*() 함수들이
// 개별 초기화 경로에서 syncMenuCheck 를 호출하므로 여기서는 `toggle-source`
// 처럼 초기화 경로에 훅이 없던 항목만 보완한다.
(function () {
  syncMenuCheck('toggle-source', !!markdownMode);
})();
// 네이티브 녹색 버튼으로 전체 화면이 바뀌면 창 리사이즈가 발생하므로,
// 리사이즈 시 Tauri 쪽에 상태를 다시 물어 메뉴 체크 + body 클래스를 맞춰
// 둔다. 전체 화면에서는 traffic-light(빨·노·초) 가 숨어 사이드바 좌측의
// 예약 패딩(~88px) 이 빈 공간으로 남아 보이는데, `body.is-fullscreen` 이
// 걸리면 CSS 가 해당 패딩을 자동 회수한다.
async function syncFullscreenState() {
  try {
    const on = await invoke('is_window_fullscreen');
    syncMenuCheck('toggle-fullscreen', !!on);
    document.body.classList.toggle('is-fullscreen', !!on);
  } catch {}
}
window.addEventListener('resize', syncFullscreenState);
// 시작 시에도 한 번 맞춰 둔다.
syncFullscreenState();

/* ─────────── Find & Replace bar (Cmd/Ctrl+F) ─────────── */
// 에디터 상단에 떠 있는 검색·바꾸기 바. `committed` 버퍼를 직접 조작하고
// `render()` 로 재렌더하므로 contenteditable DOM 을 건드리지 않는다.
//
// • Cmd/Ctrl+F — 바 열기 + 찾기 입력 포커스
// • Enter / ⇧Enter — 다음 / 이전 일치
// • Cmd/Ctrl+G — 다음 일치 (macOS 표준)
// • Esc — 바 닫기
// • ⇄ 버튼 또는 Alt+Cmd/Ctrl+F — 바꾸기 섹션 열기
const findState = {
  query: '',
  matches: [],
  cursor: -1,      // 현재 강조 중인 matches 인덱스
  caseSensitive: false,
};
function escapeRegex(s) { return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); }
function ensureFindBar() {
  let bar = document.getElementById('find-bar');
  if (bar) return bar;
  bar = document.createElement('div');
  bar.id = 'find-bar';
  bar.className = 'find-bar hidden';
  bar.innerHTML = `
    <div class="find-row">
      <input id="find-input" type="search" placeholder="찾기" autocomplete="off" spellcheck="false" />
      <span id="find-count" class="find-count">0/0</span>
      <button id="find-case" class="find-btn find-toggle" title="대소문자 구분">Aa</button>
      <button id="find-prev" class="find-btn" title="이전 (⇧Enter)" aria-label="이전">↑</button>
      <button id="find-next" class="find-btn" title="다음 (Enter)" aria-label="다음">↓</button>
      <button id="find-toggle-replace" class="find-btn" title="바꾸기 열기/닫기" aria-label="바꾸기 전환">⇄</button>
      <button id="find-close" class="find-btn" title="닫기 (Esc)" aria-label="닫기">×</button>
    </div>
    <div class="find-row find-replace-row hidden">
      <input id="replace-input" type="text" placeholder="바꾸기" autocomplete="off" spellcheck="false" />
      <button id="replace-one" class="find-btn find-btn-wide" title="현재 항목 바꾸기">바꾸기</button>
      <button id="replace-all" class="find-btn find-btn-wide" title="모두 바꾸기">모두</button>
    </div>
  `;
  document.body.appendChild(bar);
  const $ = (id) => document.getElementById(id);
  $('find-input').addEventListener('input', (ev) => {
    // 검색어가 바뀔 때마다 render() 를 부르면 editor 의 selection 이
    // 다시 설정되면서 브라우저가 focus 를 editor 로 옮겨 가고, 다음 키
    // 입력이 editor 로 새어 들어간다. 그래서 입력 중에는 매칭 개수만
    // 다시 계산하고 cursor/render 는 건드리지 않는다. 실제 점프는 Enter
    // / 다음·이전 버튼 / Cmd+G 에서 수행한다.
    updateFindMatches(ev.target.value);
  });
  $('find-input').addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') { ev.preventDefault(); findNextMatch(ev.shiftKey ? -1 : 1); }
    else if (ev.key === 'Escape') { ev.preventDefault(); closeFindBar(); }
  });
  $('replace-input').addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') { ev.preventDefault(); doReplaceOne(); }
    else if (ev.key === 'Escape') { ev.preventDefault(); closeFindBar(); }
  });
  $('find-prev').addEventListener('click', () => findNextMatch(-1));
  $('find-next').addEventListener('click', () => findNextMatch(1));
  $('find-close').addEventListener('click', closeFindBar);
  $('find-toggle-replace').addEventListener('click', () => {
    const row = bar.querySelector('.find-replace-row');
    row.classList.toggle('hidden');
    if (!row.classList.contains('hidden')) $('replace-input').focus();
  });
  $('find-case').addEventListener('click', () => {
    findState.caseSensitive = !findState.caseSensitive;
    $('find-case').classList.toggle('find-toggle-on', findState.caseSensitive);
    updateFindMatches($('find-input').value);
    highlightCurrent();
  });
  $('replace-one').addEventListener('click', doReplaceOne);
  $('replace-all').addEventListener('click', doReplaceAll);
  return bar;
}
function findBarOpen() {
  const bar = document.getElementById('find-bar');
  return !!bar && !bar.classList.contains('hidden');
}
function openFindBar(withReplace) {
  const bar = ensureFindBar();
  bar.classList.remove('hidden');
  const replaceRow = bar.querySelector('.find-replace-row');
  if (withReplace) replaceRow.classList.remove('hidden');
  const input = document.getElementById('find-input');
  // 현재 선택된 텍스트가 있으면 그걸 기본 검색어로.
  const sel = (window.getSelection() || { toString: () => '' }).toString();
  if (sel && sel.trim()) input.value = sel.trim();
  input.focus();
  input.select();
  updateFindMatches(input.value);
  highlightCurrent();
}
function closeFindBar() {
  const bar = document.getElementById('find-bar');
  if (bar) bar.classList.add('hidden');
  findState.matches = [];
  findState.cursor = -1;
  rebuildFindHighlights(); // clears highlights
  if (typeof editor !== 'undefined' && editor) editor.focus();
}
function updateFindMatches(q) {
  findState.query = q;
  findState.matches = [];
  findState.cursor = -1;
  if (!q) { renderFindCount(); rebuildFindHighlights(); return; }
  const src = typeof committed === 'string' ? committed : '';
  const flags = findState.caseSensitive ? 'g' : 'gi';
  const re = new RegExp(escapeRegex(q), flags);
  let m;
  while ((m = re.exec(src)) !== null) {
    findState.matches.push(m.index);
    if (re.lastIndex === m.index) re.lastIndex++;
  }
  // 현재 커서 위치에서 가장 가까운 다음 일치를 활성으로.
  if (findState.matches.length > 0) {
    const c = typeof cursor === 'number' ? cursor : 0;
    let idx = findState.matches.findIndex((p) => p >= c);
    if (idx < 0) idx = 0;
    findState.cursor = idx;
  }
  renderFindCount();
  rebuildFindHighlights();
}

/* 검색어 매칭 하이라이팅 — CSS Custom Highlights API 를 써서 DOM 을
   바꾸지 않고 시각 강조만 추가한다. `committed` 상의 offset 을 에디터
   DOM 의 (node, offset) 로 풀어 Range 를 만들고, 'find-match' 그룹에는
   전체 매칭, 'find-match-active' 그룹에는 현재 선택된 일치를 넣는다. */
let _findHL = null;
let _findHLActive = null;
function ensureFindHighlights() {
  if (typeof CSS === 'undefined' || !CSS.highlights || typeof Highlight === 'undefined') {
    return null;
  }
  if (!_findHL) {
    _findHL = new Highlight();
    _findHLActive = new Highlight();
    CSS.highlights.set('find-match', _findHL);
    CSS.highlights.set('find-match-active', _findHLActive);
  }
  return _findHL;
}
function makeRangeFromSourceRange(start, end) {
  if (typeof sourceIdxToDom !== 'function') return null;
  const a = sourceIdxToDom(start);
  const b = sourceIdxToDom(end);
  if (!a || !b) return null;
  try {
    const r = document.createRange();
    r.setStart(a.node, a.offset);
    r.setEnd(b.node, b.offset);
    return r;
  } catch { return null; }
}
function rebuildFindHighlights() {
  const hl = ensureFindHighlights();
  if (!hl) return;
  hl.clear();
  _findHLActive.clear();
  if (!findBarOpen() || !findState.query) return;
  const q = findState.query;
  const qLen = q.length;
  findState.matches.forEach((pos, i) => {
    const r = makeRangeFromSourceRange(pos, pos + qLen);
    if (!r) return;
    if (i === findState.cursor) _findHLActive.add(r);
    else hl.add(r);
  });
}
function renderFindCount() {
  const el = document.getElementById('find-count');
  if (!el) return;
  const total = findState.matches.length;
  const current = findState.cursor >= 0 ? findState.cursor + 1 : 0;
  el.textContent = `${current}/${total}`;
  el.classList.toggle('find-count-miss', findState.query && total === 0);
}
function highlightCurrent() {
  if (findState.cursor < 0 || findState.matches.length === 0) return;
  const pos = findState.matches[findState.cursor];
  cursor = pos + findState.query.length;
  // render() 와 그 후 에디터 스크롤 조정은 에디터에 포커스를 옮길 수 있으
  // 므로, find 바가 열린 경우 직전에 포커스를 갖고 있던 엘리먼트(검색/
  // 바꾸기 입력)를 기억해 두었다가 복원한다.
  const focused = document.activeElement;
  const shouldRestore = focused && focused.closest && focused.closest('.find-bar');
  const selStart = shouldRestore ? focused.selectionStart : null;
  const selEnd = shouldRestore ? focused.selectionEnd : null;
  render();
  const ed = document.getElementById('editor');
  if (ed) {
    const text = (typeof committed === 'string' ? committed : '') || '';
    const line = text.slice(0, pos).split('\n').length - 1;
    const total = Math.max(1, text.split('\n').length);
    const ratio = Math.max(0, Math.min(1, line / total));
    const target = ratio * Math.max(0, ed.scrollHeight - ed.clientHeight) - ed.clientHeight * 0.3;
    if (target > 0) ed.scrollTop = target;
  }
  if (shouldRestore) {
    // render() 이후 포커스가 에디터로 옮겨졌을 수 있다. 한 tick 기다린 뒤
    // 원래 입력으로 포커스와 캐럿 위치를 돌려 놓아, 타이핑이 그대로 find
    // 입력에 꽂히도록 한다.
    requestAnimationFrame(() => {
      try {
        focused.focus();
        if (selStart != null && selEnd != null && typeof focused.setSelectionRange === 'function') {
          focused.setSelectionRange(selStart, selEnd);
        }
      } catch {}
    });
  }
}
function findNextMatch(dir) {
  if (findState.matches.length === 0) return;
  const n = findState.matches.length;
  findState.cursor = (findState.cursor + (dir >= 0 ? 1 : -1) + n) % n;
  highlightCurrent();
  renderFindCount();
  rebuildFindHighlights();
}
function doReplaceOne() {
  const q = findState.query;
  const rInput = document.getElementById('replace-input');
  const r = rInput ? rInput.value : '';
  if (!q || findState.cursor < 0 || findState.matches.length === 0) return;
  const pos = findState.matches[findState.cursor];
  committed = committed.slice(0, pos) + r + committed.slice(pos + q.length);
  cursor = pos + r.length;
  try { localStorage.setItem('leaf-ime:last-replace', r); } catch {}
  render();
  // 재계산 후 다음 일치로 이동.
  updateFindMatches(q);
  if (findState.matches.length > 0) highlightCurrent();
  logEvent(`1개 바꿈`);
}
function doReplaceAll() {
  const q = findState.query;
  const rInput = document.getElementById('replace-input');
  const r = rInput ? rInput.value : '';
  if (!q) return;
  const flags = findState.caseSensitive ? 'g' : 'gi';
  const re = new RegExp(escapeRegex(q), flags);
  let count = 0;
  committed = committed.replace(re, () => { count++; return r; });
  if (count > 0) {
    cursor = Math.min(cursor, committed.length);
    render();
    updateFindMatches(q);
  }
  logEvent(`${count}개 모두 바꿈`);
}
// Esc 글로벌 — 바 내부가 아닌 곳에서 눌러도 닫기.
window.addEventListener('keydown', (ev) => {
  if (ev.key === 'Escape' && findBarOpen()) {
    ev.preventDefault();
    closeFindBar();
  }
}, true);
