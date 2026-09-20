import './style.css';
import { publishSummary } from './usage-summary';

type WindowUsage = {
  label: string;
  usedPercent: number;
  leftPercent: number;
  windowDurationMins?: number | null;
  resetsAt?: number | null;
};

type Credits = {
  hasCredits?: boolean | null;
  unlimited?: boolean | null;
  balance?: string | null;
};

type Usage = {
  source: string;
  planType?: string | null;
  fetchedAt: number;
  fiveHour?: WindowUsage | null;
  weekly?: WindowUsage | null;
  otherWindows: WindowUsage[];
  credits?: Credits | null;
  ordinaryUsageAllowed?: boolean | null;
};

type Provider = 'codex' | 'cursor';
type CursorUsage = {
  authSource: 'app' | 'web';
  fetchedAt: number;
  planType?: string | null;
  windows: WindowUsage[];
  onDemandUsedUsd?: number | null;
  onDemandLimitUsd?: number | null;
};
let provider: Provider = localStorage.getItem('usage-provider') === 'cursor' ? 'cursor' : 'codex';
let cursorSource = localStorage.getItem('cursor-source') ?? 'auto';
if (!['auto', 'app', 'web'].includes(cursorSource)) cursorSource = 'auto';
let requestId = 0;

const app = document.querySelector<HTMLDivElement>('#app')!;

app.innerHTML = `
  <main class="shell">
    <header>
      <div>
        <div id="provider-name" class="eyebrow">OPENAI CODEX</div>
        <h1>Usage</h1>
      </div>
      <button id="refresh" class="refresh-button" type="button" title="Refresh usage" aria-label="Refresh usage" aria-busy="false">
        <svg class="refresh-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <g class="refresh-arrows"><path d="M20 7v5h-5M4 17v-5h5"/><path d="M6.1 6.1A8 8 0 0 1 19.8 11M4.2 13a8 8 0 0 0 13.7 4.9"/></g>
          <path class="refresh-check" d="m6 12 4 4 8-8"/>
        </svg>
        <span id="refresh-label">Refresh</span>
      </button>
    </header>

    <nav class="providers" aria-label="Usage provider">
      <button type="button" data-provider="codex" aria-pressed="true">Codex</button>
      <button type="button" data-provider="cursor" aria-pressed="false">Cursor</button>
    </nav>

    <section id="cursor-actions" class="cursor-actions hidden" aria-label="Cursor account">
      <button type="button" id="cursor-use-app">Use Cursor app</button>
      <button type="button" id="cursor-connect">Web sign-in</button>
      <button type="button" id="cursor-disconnect">Clear web sign-in</button>
    </section>

    <section id="status" class="status muted" role="status">Connecting…</section>

    <section id="meters" class="meters"></section>

    <section id="credits" class="credits hidden"></section>

    <footer>
      <span id="updated">Not updated yet</span>
      <span id="plan"></span>
    </footer>
  </main>
`;

const refreshButton = document.querySelector<HTMLButtonElement>('#refresh')!;
const refreshLabel = document.querySelector<HTMLElement>('#refresh-label')!;
let refreshReset: ReturnType<typeof setTimeout> | undefined;

function setRefreshState(state: 'idle' | 'loading' | 'success' | 'error' | 'waiting') {
  clearTimeout(refreshReset);
  const labels = { idle: 'Refresh', loading: 'Refreshing', success: 'Updated', error: 'Retry', waiting: 'Please wait' };
  refreshButton.dataset.state = state;
  refreshButton.disabled = state === 'loading' || state === 'waiting';
  refreshButton.setAttribute('aria-busy', String(refreshButton.disabled));
  refreshLabel.textContent = labels[state];
  refreshButton.setAttribute('aria-label', `${labels[state]} usage`);
  refreshButton.title = state === 'success' ? 'Usage updated successfully' : state === 'error' ? 'Refresh failed — click to retry' : `${labels[state]} usage`;
  if (state === 'success') refreshReset = setTimeout(() => setRefreshState('idle'), 1800);
}
const statusEl = document.querySelector<HTMLElement>('#status')!;
const metersEl = document.querySelector<HTMLElement>('#meters')!;
const creditsEl = document.querySelector<HTMLElement>('#credits')!;
const updatedEl = document.querySelector<HTMLElement>('#updated')!;
const planEl = document.querySelector<HTMLElement>('#plan')!;
const cursorActions = document.querySelector<HTMLElement>('#cursor-actions')!;
const connectButton = document.querySelector<HTMLButtonElement>('#cursor-connect')!;
const disconnectButton = document.querySelector<HTMLButtonElement>('#cursor-disconnect')!;
const useAppButton = document.querySelector<HTMLButtonElement>('#cursor-use-app')!;

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!));
}

function clearUsage() {
  metersEl.replaceChildren();
  creditsEl.replaceChildren();
  creditsEl.classList.add('hidden');
  updatedEl.textContent = 'Not updated yet';
  planEl.textContent = '';
  publishSummary(provider, 'loading', [], 'Connecting…');
}

function showProvider() {
  document.querySelector('#provider-name')!.textContent = provider === 'cursor' ? 'CURSOR' : 'OPENAI CODEX';
  cursorActions.classList.toggle('hidden', provider !== 'cursor');
  document.querySelectorAll<HTMLButtonElement>('[data-provider]').forEach(button => {
    button.setAttribute('aria-pressed', String(button.dataset.provider === provider));
  });
}

function clamp(n: number): number {
  return Math.max(0, Math.min(100, n));
}

function formatReset(epoch?: number | null): string {
  if (!epoch) return 'Reset time unavailable';
  const d = new Date(epoch * 1000);
  const sameDay = d.toDateString() === new Date().toDateString();
  return sameDay
    ? `Resets ${d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}`
    : `Resets ${d.toLocaleString([], { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })}`;
}

function meter(window: WindowUsage): string {
  const left = clamp(window.leftPercent);
  return `
    <article class="meter-card">
      <div class="meter-top">
        <span class="meter-label">${escapeHtml(window.label)}</span>
        <strong>${left.toFixed(left % 1 ? 1 : 0)}% left</strong>
      </div>
      <div class="bar"><div class="fill" style="width:${left}%"></div></div>
      <div class="reset">${formatReset(window.resetsAt)}</div>
    </article>
  `;
}

function render(data: Usage) {
  const primaryMeters = [data.fiveHour, data.weekly].filter(Boolean) as WindowUsage[];
  metersEl.innerHTML = primaryMeters.map(meter).join('');

  if (!primaryMeters.length && data.otherWindows.length) {
    metersEl.innerHTML = data.otherWindows.map(meter).join('');
  }

  if (!primaryMeters.length && !data.otherWindows.length) {
    metersEl.innerHTML = '<div class="empty">Codex returned no rate-limit windows for this account.</div>';
  }

  if (data.credits && (data.credits.hasCredits || data.credits.unlimited || data.credits.balance)) {
    creditsEl.classList.remove('hidden');
    const value = data.credits.unlimited ? 'Unlimited' : (data.credits.balance ?? 'Available');
    creditsEl.innerHTML = `<span>Credits</span><strong>${escapeHtml(value)}</strong>`;
  } else {
    creditsEl.classList.add('hidden');
  }

  statusEl.textContent = data.ordinaryUsageAllowed === false
    ? 'Ordinary Codex usage is currently limited.'
    : 'Live data from your local Codex sign-in';
  statusEl.className = data.ordinaryUsageAllowed === false ? 'status warning' : 'status ok';

  updatedEl.textContent = `Updated ${new Date(data.fetchedAt * 1000).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}`;
  planEl.textContent = data.planType ? data.planType.split('_').join(' ') : '';
  publishSummary(provider, 'ready', primaryMeters.length ? primaryMeters : data.otherWindows, statusEl.textContent ?? '');
}

function renderCursor(data: CursorUsage) {
  if (data.authSource === 'app' && cursorSource === 'auto') {
    cursorSource = 'app';
    localStorage.setItem('cursor-source', cursorSource);
  }
  metersEl.innerHTML = data.windows.length
    ? data.windows.map(meter).join('')
    : '<div class="empty">Cursor did not return an included allowance for this account.</div>';
  if (data.onDemandUsedUsd != null) {
    const dollars = new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' });
    const used = dollars.format(data.onDemandUsedUsd);
    const limit = data.onDemandLimitUsd != null && data.onDemandLimitUsd > 0
      ? ` / ${dollars.format(data.onDemandLimitUsd)}` : '';
    creditsEl.innerHTML = `<span>On-demand spent</span><strong>${used}${limit}</strong>`;
    creditsEl.classList.remove('hidden');
  } else {
    creditsEl.classList.add('hidden');
  }
  statusEl.textContent = data.authSource === 'app'
    ? 'Connected through your Cursor app sign-in'
    : 'Connected through your Cursor web sign-in';
  statusEl.className = 'status ok';
  updatedEl.textContent = `Updated ${new Date(data.fetchedAt * 1000).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}`;
  planEl.textContent = data.planType?.split('_').join(' ') ?? '';
  publishSummary(provider, 'ready', data.windows, data.windows.length ? statusEl.textContent ?? '' : 'No included allowance reported');
}

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function refresh(force = false) {
  const currentRequest = ++requestId;
  const currentProvider = provider;
  setRefreshState('loading');
  let succeeded = false;
  statusEl.textContent = `Reading ${currentProvider === 'cursor' ? 'Cursor' : 'Codex'} usage…`;
  statusEl.className = 'status muted';
  publishSummary(currentProvider, 'loading', [], 'Refreshing…');

  try {
    if (!isTauriRuntime()) {
      throw new Error('This page is running in a normal browser. Start it with `npm run tauri dev` and use the separate Codex Usage desktop window, not localhost in your browser.');
    }

    const { invoke } = await import('@tauri-apps/api/core');
    if (currentProvider === 'cursor') {
      const data = await invoke<CursorUsage>('get_cursor_usage', { force, source: cursorSource });
      if (currentRequest === requestId) renderCursor(data);
    } else {
      const data = await invoke<Usage>('get_codex_usage', { force });
      if (currentRequest === requestId) render(data);
    }
    succeeded = true;
  } catch (error) {
    if (currentRequest !== requestId) return;
    clearUsage();
    statusEl.textContent = String(error);
    statusEl.className = 'status error';
    publishSummary(currentProvider, 'error', [], String(error));
    metersEl.innerHTML = currentProvider === 'cursor'
      ? '<div class="empty">Already signed in to the Cursor app? Click <strong>Use Cursor app</strong>. For a separate account, choose <strong>Web sign-in</strong>, finish signing in, then refresh.</div>'
      : `
      <div class="empty">
        Make sure <code>codex</code> is installed, on PATH, and signed in with ChatGPT.
      </div>`;
  } finally {
    if (currentRequest === requestId) {
      setRefreshState(succeeded ? 'success' : 'error');
    }
  }
}

refreshButton.addEventListener('click', () => refresh(true));

document.querySelectorAll<HTMLButtonElement>('[data-provider]').forEach(button => {
  button.addEventListener('click', () => {
    provider = button.dataset.provider as Provider;
    localStorage.setItem('usage-provider', provider);
    clearUsage();
    showProvider();
    void refresh();
  });
});

async function cursorAccountAction(command: 'connect_cursor' | 'disconnect_cursor') {
  const accountRequest = ++requestId;
  setRefreshState('waiting');
  connectButton.disabled = disconnectButton.disabled = useAppButton.disabled = true;
  clearUsage();
  try {
    if (!isTauriRuntime()) throw new Error('Open the desktop widget to sign in to Cursor.');
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke(command);
    cursorSource = command === 'connect_cursor' ? 'web' : 'auto';
    localStorage.setItem('cursor-source', cursorSource);
    if (provider === 'cursor') {
      statusEl.className = 'status muted';
      statusEl.textContent = command === 'connect_cursor'
        ? 'Complete Cursor sign-in, close the sign-in window, then click Refresh.'
        : 'Web sign-in cleared. Your Cursor app sign-in is unchanged. Refresh to reconnect.';
      publishSummary('cursor', 'loading', [], statusEl.textContent);
    }
  } catch (error) {
    if (provider === 'cursor') {
      statusEl.className = 'status error';
      statusEl.textContent = String(error);
      publishSummary('cursor', 'error', [], String(error));
    }
  } finally {
    connectButton.disabled = disconnectButton.disabled = useAppButton.disabled = false;
    if (accountRequest === requestId) setRefreshState('idle');
  }
}
connectButton.addEventListener('click', () => void cursorAccountAction('connect_cursor'));
disconnectButton.addEventListener('click', () => void cursorAccountAction('disconnect_cursor'));
useAppButton.addEventListener('click', () => {
  cursorSource = 'app';
  localStorage.setItem('cursor-source', cursorSource);
  clearUsage();
  void refresh(true);
});

if (isTauriRuntime()) {
  import('@tauri-apps/api/event').then(({ listen }) => {
    void listen('request-refresh', () => refresh(true));
  }).catch(() => undefined);
}

showProvider();
refresh(false);
setInterval(() => refresh(false), 5 * 60 * 1000);
