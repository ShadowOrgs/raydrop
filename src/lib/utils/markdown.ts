import { marked } from 'marked';

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function highlightCode(code: string): string {
  let escaped = escapeHtml(code);
  // Highlight comments (// ... or # ... or /* ... */)
  escaped = escaped.replace(
    /(\/\/[^\n]*|\/\*[\s\S]*?\*\/|#[^\n]*)/g,
    '<span class="tok-com">$1</span>'
  );
  // Highlight strings
  escaped = escaped.replace(
    /(&quot;.*?&quot;|&#039;.*?&#039;|`[^`]*`)/g,
    '<span class="tok-str">$1</span>'
  );
  // Highlight numbers
  escaped = escaped.replace(
    /\b(\d+(?:\.\d+)?)\b/g,
    '<span class="tok-num">$1</span>'
  );
  // Highlight common programming keywords
  escaped = escaped.replace(
    /\b(const|let|var|function|fn|pub|struct|enum|impl|match|if|else|for|while|loop|return|async|await|import|export|from|type|interface|class|def|try|catch|true|false|null|undefined|None|Some|Ok|Err|self|mut|use|mod)\b/g,
    '<span class="tok-kw">$1</span>'
  );
  return escaped;
}

const renderer = new marked.Renderer();
renderer.code = ({ text, lang }: { text: string; lang?: string }) => {
  const language = (lang || 'code').trim();
  const highlighted = highlightCode(text);
  const encodedRaw = encodeURIComponent(text);
  return `
    <div class="raydrop-code-block my-2 rounded-xl overflow-hidden border border-slate-700/80 bg-slate-950/90 shadow-md">
      <div class="flex items-center justify-between px-3 py-1.5 bg-slate-900/90 border-b border-slate-800 text-[10px] font-mono text-slate-400">
        <span class="uppercase tracking-wider font-semibold text-cyan-400">${escapeHtml(language)}</span>
        <button
          type="button"
          class="px-2 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white transition cursor-pointer"
          onclick="navigator.clipboard.writeText(decodeURIComponent('${encodedRaw}')); this.textContent='Copied!'; setTimeout(() => this.textContent='Copy', 1500);"
        >Copy</button>
      </div>
      <pre class="p-3 overflow-x-auto text-[11px] sm:text-xs font-mono leading-relaxed text-slate-200"><code>${highlighted}</code></pre>
    </div>
  `;
};

marked.setOptions({
  gfm: true,
  breaks: true,
  renderer,
});

export function renderMarkdown(content: string): string {
  if (!content) return '';
  try {
    return marked.parse(content) as string;
  } catch (e) {
    return escapeHtml(content);
  }
}
