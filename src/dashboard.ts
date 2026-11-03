import './style.css';
import { publishSummary } from './usage-summary';
import { loadSettings, saveSettings, DEFAULT_SETTINGS, type WidgetSettings } from './settings';

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

type ResetCredit = {
  availableCount: number;
  description?: string | null;
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
  resetCredits?: ResetCredit | null;
  accountId?: string | null;
};

type Provider = 'codex' | 'cursor';

type CursorUsage = {
  authSource: 'app' | 'web';
  fetchedAt: number;
  planType?: string | null;
  resetsAt?: number | null;
  billingCycleStart?: number | null;
  windows: WindowUsage[];
  onDemandUsedUsd?: number | null;
  onDemandLimitUsd?: number | null;
  totalRequestsUsed?: number | null;
  totalRequestsLimit?: number | null;
  totalRequestsRemaining?: number | null;
  bonusRequests?: number | null;
  autoModelDisplayMessage?: string | null;
  namedModelDisplayMessage?: string | null;
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
        <div class="eyebrow-wrap">
          <span class="live-dot" aria-hidden="true"></span>
          <span id="provider-name" class="eyebrow">OPENAI CODEX</span>
        </div>
        <h1>Usage</h1>
      </div>
      <div class="header-actions">
        <button id="settings-btn" class="settings-toggle-btn" type="button" title="Status Bar Settings" aria-label="Status Bar Settings">
          <svg class="gear-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3"></circle>
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
          </svg>
        </button>
        <button id="compact-btn" class="compact-mode-btn" type="button" title="Minimize to top status bar" aria-label="Minimize to status bar">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="4 14 10 14 10 20"></polyline>
            <polyline points="20 10 14 10 14 4"></polyline>
            <line x1="14" y1="10" x2="21" y2="3"></line>
            <line x1="3" y1="21" x2="10" y2="14"></line>
          </svg>
          <span>Compact</span>
        </button>
        <button id="refresh" class="refresh-button" type="button" title="Refresh usage" aria-label="Refresh usage" aria-busy="false">
          <svg class="refresh-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <g class="refresh-arrows"><path d="M20 7v5h-5M4 17v-5h5"/><path d="M6.1 6.1A8 8 0 0 1 19.8 11M4.2 13a8 8 0 0 0 13.7 4.9"/></g>
            <path class="refresh-check" d="m6 12 4 4 8-8"/>
          </svg>
          <span id="refresh-label">Refresh</span>
        </button>
      </div>
    </header>

    <nav class="providers" aria-label="Usage provider">
      <button type="button" data-provider="codex" aria-pressed="true">
        <svg class="provider-tab-icon codex" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 2L14.4 9.6L22 12L14.4 14.4L12 22L9.6 14.4L2 12L9.6 9.6L12 2Z"/>
        </svg>
        <span>Codex</span>
      </button>
      <button type="button" data-provider="cursor" aria-pressed="false">
        <svg class="provider-tab-icon cursor" viewBox="0 0 24 24" fill="currentColor">
          <path d="M4 3L17 12L11 13.5L14.5 20.5L12 21.5L8.5 14.5L4 18V3Z"/>
        </svg>
        <span>Cursor</span>
      </button>
    </nav>

    <section id="cursor-actions" class="cursor-actions hidden" aria-label="Cursor account">
      <button type="button" id="cursor-use-app">
        <svg class="action-mini-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>
        <span>Use Cursor app</span>
      </button>
      <button type="button" id="cursor-connect">
        <svg class="action-mini-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg>
        <span>Web sign-in</span>
      </button>
      <button type="button" id="cursor-disconnect">
        <svg class="action-mini-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6L6 18M6 6l12 12"/></svg>
        <span>Clear web sign-in</span>
      </button>
    </section>

    <section id="status" class="status muted" role="status">Connecting…</section>

    <section id="meters" class="meters"></section>

    <section id="credits" class="credits hidden"></section>

    <footer>
      <div class="footer-left">
        <span class="live-status-indicator" title="Live status"></span>
        <span id="updated">Not updated yet</span>
      </div>
      <span id="plan"></span>
    </footer>

    <div id="settings-modal" class="settings-modal hidden" role="dialog" aria-modal="true" aria-labelledby="settings-heading">
      <div class="settings-backdrop" id="settings-backdrop"></div>
      <div class="settings-dialog">
        <div class="settings-header">
          <div class="settings-title-wrap">
            <svg class="settings-title-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="3"></circle>
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
            </svg>
            <h2 id="settings-heading">Status Bar Settings</h2>
          </div>
          <button id="settings-close" class="settings-close-btn" type="button" aria-label="Close settings">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6L6 18M6 6l12 12"/></svg>
          </button>
        </div>

        <div class="settings-body">
          <div class="settings-section">
            <span class="settings-section-title">Screen Placement</span>

            <div class="setting-item">
              <div class="setting-row">
                <span class="setting-label">Position</span>
                <span class="setting-badge" id="val-position">Top</span>
              </div>
              <p class="setting-help">Placement on screen. Left and Right use a compact vertical UI design.</p>
              <div class="position-segmented" role="radiogroup" aria-label="Status Bar Position">
                <button type="button" class="pos-btn active" data-pos="top">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 6h16M12 10v8M9 13l3-3 3 3"/></svg>
                  <span>Top</span>
                </button>
                <button type="button" class="pos-btn" data-pos="bottom">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 18h16M12 6v8M9 11l3 3 3-3"/></svg>
                  <span>Bottom</span>
                </button>
                <button type="button" class="pos-btn" data-pos="left">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4v16M10 12h8M13 9l-3 3 3 3"/></svg>
                  <span>Left (Vert)</span>
                </button>
                <button type="button" class="pos-btn" data-pos="right">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 4v16M6 12h8M11 9l3 3-3 3"/></svg>
                  <span>Right (Vert)</span>
                </button>
              </div>
            </div>
          </div>

          <div class="settings-divider"></div>

          <div class="settings-section">
            <span class="settings-section-title">Opacity & Click-Through</span>

            <div class="setting-item">
              <div class="setting-row">
                <label for="setting-normal-opacity" class="setting-label">Mouse Leave Opacity</label>
                <span class="setting-badge" id="val-normal-opacity">95%</span>
              </div>
              <p class="setting-help">Default opacity of status bar when mouse is not hovering</p>
              <input type="range" id="setting-normal-opacity" min="20" max="100" step="5" value="95">
            </div>

            <div class="setting-item">
              <div class="setting-row">
                <label for="setting-hover-opacity" class="setting-label">Mouse Enter Opacity</label>
                <span class="setting-badge" id="val-hover-opacity">25%</span>
              </div>
              <p class="setting-help">Opacity decreases when mouse enters so you can see behind it</p>
              <input type="range" id="setting-hover-opacity" min="5" max="100" step="5" value="25">
            </div>

            <div class="setting-toggle-item">
              <label class="toggle-control" for="setting-click-through">
                <input type="checkbox" id="setting-click-through">
                <span class="toggle-slider"></span>
                <div class="toggle-text">
                  <span class="toggle-title">Click-Through Status Bar</span>
                  <span class="toggle-help">Pass mouse clicks completely through to windows underneath</span>
                </div>
              </label>
            </div>
          </div>

          <div class="settings-divider"></div>

          <div class="settings-section">
            <span class="settings-section-title">Window Controls</span>

            <div class="setting-toggle-item">
              <label class="toggle-control" for="setting-draggable">
                <input type="checkbox" id="setting-draggable">
                <span class="toggle-slider"></span>
                <div class="toggle-text">
                  <span class="toggle-title">Draggable Status Bar</span>
                  <span class="toggle-help">Allow clicking and dragging status bar anywhere on screen</span>
                </div>
              </label>
            </div>

            <div class="setting-toggle-item">
              <label class="toggle-control" for="setting-click-to-open">
                <input type="checkbox" id="setting-click-to-open">
                <span class="toggle-slider"></span>
                <div class="toggle-text">
                  <span class="toggle-title">Click to Open Code Usage Window</span>
                  <span class="toggle-help">Show expand button and open full window when clicked</span>
                </div>
              </label>
            </div>
          </div>
        </div>

        <div class="settings-footer">
          <button type="button" id="settings-reset" class="settings-btn-secondary">Reset Defaults</button>
          <button type="button" id="settings-done" class="settings-btn-primary">Done</button>
        </div>
      </div>
    </div>
  </main>
`;

const refreshButton = document.querySelector<HTMLButtonElement>('#refresh')!;
const refreshLabel = document.querySelector<HTMLElement>('#refresh-label')!;
const compactBtn = document.querySelector<HTMLButtonElement>('#compact-btn')!;
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

let widgetSettings: WidgetSettings = loadSettings();

const settingsBtn = document.querySelector<HTMLButtonElement>('#settings-btn')!;
const settingsModal = document.querySelector<HTMLElement>('#settings-modal')!;
const settingsBackdrop = document.querySelector<HTMLElement>('#settings-backdrop')!;
const settingsClose = document.querySelector<HTMLButtonElement>('#settings-close')!;
const settingsDone = document.querySelector<HTMLButtonElement>('#settings-done')!;
const settingsReset = document.querySelector<HTMLButtonElement>('#settings-reset')!;

const normalOpacityInput = document.querySelector<HTMLInputElement>('#setting-normal-opacity')!;
const hoverOpacityInput = document.querySelector<HTMLInputElement>('#setting-hover-opacity')!;
const valNormalOpacity = document.querySelector<HTMLElement>('#val-normal-opacity')!;
const valHoverOpacity = document.querySelector<HTMLElement>('#val-hover-opacity')!;
const draggableInput = document.querySelector<HTMLInputElement>('#setting-draggable')!;
const clickToOpenInput = document.querySelector<HTMLInputElement>('#setting-click-to-open')!;
const clickThroughInput = document.querySelector<HTMLInputElement>('#setting-click-through')!;

function syncSettingsUI() {
  const valPos = document.querySelector<HTMLElement>('#val-position');
  if (valPos) {
    const labels: Record<string, string> = {
      top: 'Top (Horizontal)',
      bottom: 'Bottom (Horizontal)',
      left: 'Left (Vertical)',
      right: 'Right (Vertical)',
    };
    valPos.textContent = labels[widgetSettings.position] || 'Top (Horizontal)';
  }
  document.querySelectorAll<HTMLButtonElement>('.pos-btn').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.pos === widgetSettings.position);
  });
  normalOpacityInput.value = String(widgetSettings.normalOpacity);
  hoverOpacityInput.value = String(widgetSettings.hoverOpacity);
  valNormalOpacity.textContent = `${widgetSettings.normalOpacity}%`;
  valHoverOpacity.textContent = `${widgetSettings.hoverOpacity}%`;
  draggableInput.checked = widgetSettings.draggable;
  clickToOpenInput.checked = widgetSettings.clickToOpen;
  clickThroughInput.checked = widgetSettings.clickThrough;
}

async function persistAndEmitSettings() {
  saveSettings(widgetSettings);
  if (isTauriRuntime()) {
    try {
      const { emit } = await import('@tauri-apps/api/event');
      await emit('widget-settings-changed', widgetSettings);
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('set_status_bar_position', { position: widgetSettings.position });
      await invoke('set_status_bar_click_through', { ignore: widgetSettings.clickThrough });
    } catch {}
  }
}

document.querySelectorAll<HTMLButtonElement>('.pos-btn').forEach(btn => {
  btn.addEventListener('click', () => {
    const pos = btn.dataset.pos as WidgetSettings['position'];
    if (pos && ['top', 'bottom', 'left', 'right'].includes(pos)) {
      widgetSettings.position = pos;
      syncSettingsUI();
      void persistAndEmitSettings();
    }
  });
});

normalOpacityInput.addEventListener('input', () => {
  widgetSettings.normalOpacity = Number(normalOpacityInput.value);
  valNormalOpacity.textContent = `${widgetSettings.normalOpacity}%`;
  void persistAndEmitSettings();
});

hoverOpacityInput.addEventListener('input', () => {
  widgetSettings.hoverOpacity = Number(hoverOpacityInput.value);
  valHoverOpacity.textContent = `${widgetSettings.hoverOpacity}%`;
  void persistAndEmitSettings();
});

draggableInput.addEventListener('change', () => {
  widgetSettings.draggable = draggableInput.checked;
  void persistAndEmitSettings();
});

clickToOpenInput.addEventListener('change', () => {
  widgetSettings.clickToOpen = clickToOpenInput.checked;
  void persistAndEmitSettings();
});

clickThroughInput.addEventListener('change', () => {
  widgetSettings.clickThrough = clickThroughInput.checked;
  void persistAndEmitSettings();
});

settingsReset.addEventListener('click', () => {
  widgetSettings = { ...DEFAULT_SETTINGS };
  syncSettingsUI();
  void persistAndEmitSettings();
});

function openSettings() {
  syncSettingsUI();
  settingsModal.classList.remove('hidden');
}

function closeSettings() {
  settingsModal.classList.add('hidden');
}

settingsBtn.addEventListener('click', openSettings);
settingsClose.addEventListener('click', closeSettings);
settingsBackdrop.addEventListener('click', closeSettings);
settingsDone.addEventListener('click', closeSettings);
syncSettingsUI();

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
  const nowMs = Date.now();
  const resetMs = epoch * 1000;
  const d = new Date(resetMs);
  const diffMs = resetMs - nowMs;

  let countdown = '';
  if (diffMs > 0) {
    const mins = Math.floor(diffMs / 60000);
    const hours = Math.floor(mins / 60);
    const days = Math.floor(hours / 24);
    if (days > 0) {
      countdown = `in ${days}d ${hours % 24}h`;
    } else if (hours > 0) {
      countdown = `in ${hours}h ${mins % 60}m`;
    } else {
      countdown = `in ${Math.max(1, mins)}m`;
    }
  }

  const sameDay = d.toDateString() === new Date().toDateString();
  const timeStr = d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  const dateStr = sameDay
    ? timeStr
    : d.toLocaleString([], { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });

  return countdown ? `Resets ${countdown} (${dateStr})` : `Resets ${dateStr}`;
}

function getWindowIcon(label: string): string {
  const lower = label.toLowerCase();
  if (lower.includes('5 hour') || lower.includes('5-hour') || lower.includes('hour')) {
    return `<svg class="meter-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 15 14"/></svg>`;
  }
  if (lower.includes('week')) {
    return `<svg class="meter-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="18" rx="2" ry="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/></svg>`;
  }
  return `<svg class="meter-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/></svg>`;
}

function meter(window: WindowUsage, subtext?: string): string {
  const left = clamp(window.leftPercent);
  const fillClass = left <= 15 ? 'fill-low' : left <= 40 ? 'fill-medium' : 'fill-high';
  const badgeClass = left <= 15 ? 'low' : left <= 40 ? 'medium' : 'high';
  const icon = getWindowIcon(window.label);

  return `
    <article class="meter-card">
      <div class="meter-top">
        <div class="meter-title-wrap">
          <span class="meter-icon-wrap">${icon}</span>
          <span class="meter-label">${escapeHtml(window.label)}</span>
        </div>
        <div class="badge-percent ${badgeClass}">
          <span class="percent-val">${left.toFixed(left % 1 ? 1 : 0)}%</span>
          <span class="percent-unit">left</span>
        </div>
      </div>
      <div class="bar"><div class="fill ${fillClass}" style="width:${left}%"></div></div>
      <div class="meter-bottom">
        <span class="reset-badge">
          <svg class="clock-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10"></circle>
            <polyline points="12 6 12 12 16 14"></polyline>
          </svg>
          <span>${formatReset(window.resetsAt)}</span>
        </span>
        ${subtext ? `<span class="meter-sub">${escapeHtml(subtext)}</span>` : ''}
      </div>
    </article>
  `;
}

function render(data: Usage) {
  const primaryMeters = [data.fiveHour, data.weekly].filter(Boolean) as WindowUsage[];
  metersEl.innerHTML = primaryMeters.map(w => meter(w)).join('');

  if (!primaryMeters.length && data.otherWindows.length) {
    metersEl.innerHTML = data.otherWindows.map(w => meter(w)).join('');
  }

  if (!primaryMeters.length && !data.otherWindows.length) {
    metersEl.innerHTML = '<div class="empty">Codex returned no rate-limit windows for this account.</div>';
  }

  const badges: string[] = [];

  if (data.resetCredits && data.resetCredits.availableCount > 0) {
    const count = data.resetCredits.availableCount;
    const title = data.resetCredits.description || 'Full rate limit reset';
    badges.push(`
      <div class="badge-card reset-credit-badge">
        <span class="badge-icon">🎁</span>
        <div class="badge-info">
          <strong>${count} free rate-limit reset${count > 1 ? 's' : ''} available</strong>
          <div class="badge-sub">${escapeHtml(title)}</div>
        </div>
      </div>
    `);
  }

  if (data.credits && (data.credits.hasCredits || data.credits.unlimited || data.credits.balance)) {
    const value = data.credits.unlimited ? 'Unlimited' : (data.credits.balance ?? 'Available');
    badges.push(`
      <div class="badge-card">
        <span>Codex credits</span>
        <strong>${escapeHtml(value)}</strong>
      </div>
    `);
  }

  if (badges.length) {
    creditsEl.classList.remove('hidden');
    creditsEl.innerHTML = badges.join('');
  } else {
    creditsEl.classList.add('hidden');
  }

  statusEl.textContent = data.ordinaryUsageAllowed === false
    ? 'Ordinary Codex usage is currently limited.'
    : 'Live rate limits from your local Codex sign-in';
  statusEl.className = data.ordinaryUsageAllowed === false ? 'status warning' : 'status ok';

  updatedEl.textContent = `Updated ${new Date(data.fetchedAt * 1000).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}`;
  planEl.innerHTML = data.planType ? `<span class="plan-tag">CHATGPT ${escapeHtml(data.planType.split('_').join(' ').toUpperCase())}</span>` : '';

  const windowsForSummary = (primaryMeters.length ? primaryMeters : data.otherWindows).map(w => ({
    label: w.label,
    leftPercent: w.leftPercent,
    resetsAt: w.resetsAt,
    subtext: formatReset(w.resetsAt),
  }));
  publishSummary(
    provider,
    'ready',
    windowsForSummary,
    statusEl.textContent ?? '',
    {
      planType: data.planType,
      resetCreditsCount: data.resetCredits?.availableCount ?? 0,
    }
  );
}

function renderCursor(data: CursorUsage) {
  if (data.authSource === 'app' && cursorSource === 'auto') {
    cursorSource = 'app';
    localStorage.setItem('cursor-source', cursorSource);
  }

  if (data.windows.length) {
    metersEl.innerHTML = data.windows.map(w => {
      let subtext: string | undefined;
      if (w.label.includes('Cursor') && data.totalRequestsUsed != null) {
        if (data.totalRequestsLimit != null && data.totalRequestsLimit > 0) {
          subtext = `${data.totalRequestsUsed.toLocaleString()} / ${data.totalRequestsLimit.toLocaleString()} requests`;
        } else {
          subtext = `${data.totalRequestsUsed.toLocaleString()} requests used`;
        }
      }
      return meter(w, subtext);
    }).join('');
  } else {
    metersEl.innerHTML = '<div class="empty">Cursor did not return an included allowance for this account.</div>';
  }

  const badges: string[] = [];

  if (data.totalRequestsRemaining != null || data.totalRequestsUsed != null) {
    const remaining = data.totalRequestsRemaining != null ? `${data.totalRequestsRemaining.toLocaleString()} remaining` : '';
    const used = data.totalRequestsUsed != null ? `${data.totalRequestsUsed.toLocaleString()} used` : '';
    const limit = data.totalRequestsLimit != null ? `of ${data.totalRequestsLimit.toLocaleString()}` : '';
    badges.push(`
      <div class="badge-card">
        <span>Included requests</span>
        <strong>${remaining || used} ${limit}</strong>
      </div>
    `);
  }

  if (data.bonusRequests && data.bonusRequests > 0) {
    badges.push(`
      <div class="badge-card">
        <span>Bonus requests</span>
        <strong>+${data.bonusRequests.toLocaleString()}</strong>
      </div>
    `);
  }

  if (data.onDemandUsedUsd != null) {
    const dollars = new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' });
    const used = dollars.format(data.onDemandUsedUsd);
    const limit = data.onDemandLimitUsd != null && data.onDemandLimitUsd > 0
      ? ` / ${dollars.format(data.onDemandLimitUsd)}` : '';
    badges.push(`
      <div class="badge-card">
        <span>On-demand spent</span>
        <strong>${used}${limit}</strong>
      </div>
    `);
  }

  if (badges.length) {
    creditsEl.classList.remove('hidden');
    creditsEl.innerHTML = badges.join('');
  } else {
    creditsEl.classList.add('hidden');
  }

  statusEl.textContent = data.authSource === 'app'
    ? 'Connected through your Cursor app sign-in'
    : 'Connected through your Cursor web sign-in';
  statusEl.className = 'status ok';

  updatedEl.textContent = `Updated ${new Date(data.fetchedAt * 1000).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}`;
  planEl.innerHTML = data.planType ? `<span class="plan-tag">CURSOR ${escapeHtml(data.planType.toUpperCase())}</span>` : '';

  const windowsForSummary = data.windows.map(w => ({
    label: w.label,
    leftPercent: w.leftPercent,
    resetsAt: data.resetsAt,
    subtext: formatReset(data.resetsAt),
  }));
  publishSummary(
    provider,
    'ready',
    windowsForSummary,
    data.windows.length ? statusEl.textContent ?? '' : 'No included allowance reported',
    {
      planType: data.planType,
    }
  );
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

compactBtn.addEventListener('click', async () => {
  if (!isTauriRuntime()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('enter_compact_mode');
  } catch (err) {
    console.error('Could not enter compact mode:', err);
  }
});

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
