import './compact.css';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Summary } from './usage-summary';
import { loadSettings, saveSettings, type WidgetSettings } from './settings';

const CODEX_ICON = `<svg class="brand-svg codex" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2L14.4 9.6L22 12L14.4 14.4L12 22L9.6 14.4L2 12L9.6 9.6L12 2Z"/></svg>`;
const CURSOR_ICON = `<svg class="brand-svg cursor" viewBox="0 0 24 24" fill="currentColor"><path d="M4 3L17 12L11 13.5L14.5 20.5L12 21.5L8.5 14.5L4 18V3Z"/></svg>`;

document.querySelector('#app')!.innerHTML = `
  <div class="compact-wrapper" id="wrapper">
    <div id="pill" class="compact-pill" role="region" aria-label="Usage status bar">
      <div class="brand-badge" id="compact-brand">
        <span class="brand-icon-wrap" id="compact-icon">${CODEX_ICON}</span>
        <span class="brand-name" id="compact-provider">Codex</span>
        <span class="credits-pill hidden" id="compact-credits">🎁 <span id="compact-credits-count">0</span></span>
      </div>

      <div class="pill-divider" aria-hidden="true"></div>

      <div class="allowances" id="compact-allowances" aria-live="polite">
        <span class="message">Connecting…</span>
      </div>

      <div id="restore-btn" class="restore-action hidden" title="Open full dashboard" role="button" tabindex="0" aria-label="Open full dashboard" data-tauri-drag-region="false">
        <svg class="restore-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <polyline points="15 3 21 3 21 9"></polyline>
          <polyline points="9 21 3 21 3 15"></polyline>
          <line x1="21" y1="3" x2="14" y2="10"></line>
          <line x1="3" y1="21" x2="10" y2="14"></line>
        </svg>
      </div>
    </div>
  </div>`;

const wrapper = document.querySelector<HTMLDivElement>('#wrapper')!;
const pill = document.querySelector<HTMLDivElement>('#pill')!;
const iconWrap = document.querySelector<HTMLElement>('#compact-icon')!;
const label = document.querySelector<HTMLElement>('#compact-provider')!;
const allowances = document.querySelector<HTMLElement>('#compact-allowances')!;
const creditsPill = document.querySelector<HTMLElement>('#compact-credits')!;
const creditsCount = document.querySelector<HTMLElement>('#compact-credits-count')!;
const restoreBtn = document.querySelector<HTMLElement>('#restore-btn')!;

let currentSettings: WidgetSettings = loadSettings();
let lastSummary: Summary | null = null;
let revision = -1;
let isHovered = false;

function setHovered(hovered: boolean) {
  if (isHovered === hovered) return;
  isHovered = hovered;
  if (hovered) {
    pill.classList.add('is-hovered');
  } else {
    pill.classList.remove('is-hovered');
  }
}

// Track mouse position across the status bar viewport
document.addEventListener('mousemove', (e) => {
  const inside = e.clientX >= 0 && e.clientX <= window.innerWidth &&
                 e.clientY >= 0 && e.clientY <= window.innerHeight;
  setHovered(inside);
});

document.addEventListener('mouseenter', () => setHovered(true));
document.addEventListener('mouseleave', () => setHovered(false));
pill.addEventListener('pointerenter', () => setHovered(true));
pill.addEventListener('pointerleave', () => setHovered(false));
pill.addEventListener('mouseleave', () => setHovered(false));
document.addEventListener('visibilitychange', () => {
  if (document.hidden) setHovered(false);
});

function applySettings(settings: WidgetSettings) {
  currentSettings = settings;

  // Apply position classes and dataset
  const isVert = settings.position === 'left' || settings.position === 'right';
  if (isVert) {
    wrapper.classList.add('is-vertical');
    pill.classList.add('is-vertical');
  } else {
    wrapper.classList.remove('is-vertical');
    pill.classList.remove('is-vertical');
  }
  wrapper.dataset.position = settings.position;
  pill.dataset.position = settings.position;

  if (lastSummary) {
    render(lastSummary);
  }

  void invoke('set_status_bar_position', { position: settings.position }).catch(() => {});

  // Apply separate normal and hover opacity variables
  document.documentElement.style.setProperty('--normal-opacity', String(settings.normalOpacity / 100));
  document.documentElement.style.setProperty('--hover-opacity', String(settings.hoverOpacity / 100));

  // Click-through setting
  if (settings.clickThrough) {
    wrapper.classList.add('is-click-through');
  } else {
    wrapper.classList.remove('is-click-through');
  }
  void invoke('set_status_bar_click_through', { ignore: settings.clickThrough }).catch(() => {});

  // Draggable setting (only if click-through is not active)
  if (settings.draggable && !settings.clickThrough) {
    pill.classList.add('is-draggable');
    pill.setAttribute('data-tauri-drag-region', 'deep');
    wrapper.setAttribute('data-tauri-drag-region', 'deep');
  } else {
    pill.classList.remove('is-draggable');
    pill.removeAttribute('data-tauri-drag-region');
    wrapper.removeAttribute('data-tauri-drag-region');
  }

  // Click to open setting and expand icon visibility
  if (settings.clickToOpen && !settings.clickThrough) {
    restoreBtn.classList.remove('hidden');
    pill.classList.add('is-clickable');
  } else {
    restoreBtn.classList.add('hidden');
    pill.classList.remove('is-clickable');
  }
}

applySettings(currentSettings);

let dragStartX = 0;
let dragStartY = 0;

// Dragging logic
pill.addEventListener('mousedown', async (e) => {
  if (e.button === 0 && currentSettings.draggable && !currentSettings.clickThrough) {
    if ((e.target as HTMLElement).closest('.restore-action')) return;
    dragStartX = e.clientX;
    dragStartY = e.clientY;
    try {
      await invoke('start_status_bar_dragging');
    } catch {
      try {
        await getCurrentWindow().startDragging();
      } catch {}
    }
  }
});

// Click to restore (only if clickToOpen is enabled and clickThrough is false)
pill.addEventListener('click', async (e) => {
  if (!currentSettings.clickToOpen || currentSettings.clickThrough) return;
  if ((e.target as HTMLElement).closest('.restore-action')) return;
  if (currentSettings.draggable) {
    const moved = Math.abs(e.clientX - dragStartX) > 4 || Math.abs(e.clientY - dragStartY) > 4;
    if (moved) return;
  }
  try {
    await invoke('restore_usage_window');
  } catch {
    allowances.textContent = 'Could not restore · try tray icon';
  }
});

restoreBtn.addEventListener('click', async (e) => {
  e.stopPropagation();
  try {
    await invoke('restore_usage_window');
  } catch {
    allowances.textContent = 'Could not restore · try tray icon';
  }
});

// Right click context action: allow restoring even if left-click is disabled
pill.addEventListener('contextmenu', async (e) => {
  if (currentSettings.clickThrough) return;
  e.preventDefault();
  try {
    await invoke('restore_usage_window');
  } catch {}
});

function cleanLabel(raw: string): string {
  const lower = raw.trim().toLowerCase();
  if (lower === '5 hour' || lower === '5-hour') return '5-Hour';
  if (lower === 'weekly') return 'Weekly';
  if (lower.includes('cursor')) return 'Cursor';
  if (lower.includes('other')) return 'Other';
  return raw;
}

function formatCountdown(resetsAt?: number | null): string {
  if (!resetsAt) return '';
  const diffMs = resetsAt * 1000 - Date.now();
  if (diffMs <= 0) return 'Resetting';
  const mins = Math.floor(diffMs / 60000);
  const hours = Math.floor(mins / 60);
  const days = Math.floor(hours / 24);
  if (days > 0) return `${days}d ${hours % 24}h`;
  if (hours > 0) return `${hours}h ${mins % 60}m`;
  return `${Math.max(1, mins)}m`;
}

function render(summary: Summary) {
  lastSummary = summary;
  if (summary.revision !== undefined && summary.revision < revision) return;
  if (summary.revision !== undefined) {
    revision = summary.revision;
  }

  const isCursor = summary.provider.toLowerCase() === 'cursor';
  label.textContent = summary.provider || 'Usage';
  iconWrap.innerHTML = isCursor ? CURSOR_ICON : CODEX_ICON;
  pill.dataset.state = summary.state;

  if (summary.resetCreditsCount && summary.resetCreditsCount > 0) {
    creditsPill.classList.remove('hidden');
    creditsCount.textContent = String(summary.resetCreditsCount);
    creditsPill.title = `${summary.resetCreditsCount} free rate-limit reset${summary.resetCreditsCount > 1 ? 's' : ''} available`;
  } else {
    creditsPill.classList.add('hidden');
  }

  allowances.replaceChildren();

  if (summary.state !== 'ready' || !summary.windows.length) {
    const message = document.createElement('span');
    message.className = 'message';
    message.textContent = summary.state === 'error' ? 'Usage unavailable · right-click to open' : (summary.message || 'Connecting…');
    allowances.append(message);
    return;
  }

  for (const window of summary.windows.slice(0, 2)) {
    const left = Math.max(0, Math.min(100, window.leftPercent));
    const levelClass = left <= 15 ? 'low' : left <= 40 ? 'medium' : 'high';
    const countdown = formatCountdown(window.resetsAt);

    const item = document.createElement('div');
    item.className = 'allowance-item';
    if (countdown) {
      item.title = `${window.label}: ${left.toFixed(left % 1 ? 1 : 0)}% left (Resets in ${countdown})`;
    } else {
      item.title = `${window.label}: ${left.toFixed(left % 1 ? 1 : 0)}% left`;
    }

    const header = document.createElement('div');
    header.className = 'allowance-header';

    const name = document.createElement('span');
    name.className = 'allowance-label';
    name.textContent = cleanLabel(window.label);

    const stat = document.createElement('div');
    stat.className = 'allowance-stat';

    const percent = document.createElement('span');
    percent.className = `allowance-val ${levelClass}`;
    percent.textContent = `${left.toFixed(left % 1 ? 1 : 0)}%`;

    const unit = document.createElement('span');
    unit.className = 'allowance-unit';
    unit.textContent = 'left';

    stat.append(percent, unit);

    if (countdown) {
      const time = document.createElement('span');
      time.className = 'allowance-time allowance-time-inline';
      time.textContent = countdown;
      stat.append(time);
    }

    header.append(name, stat);

    const track = document.createElement('div');
    track.className = 'mini-track';

    const fill = document.createElement('div');
    fill.className = `mini-fill ${levelClass}`;
    fill.style.width = `${left}%`;
    track.append(fill);

    item.append(header, track);

    if (countdown) {
      const footer = document.createElement('div');
      footer.className = 'allowance-footer';
      const footerTime = document.createElement('span');
      footerTime.className = 'allowance-time';
      footerTime.textContent = countdown;
      footer.append(footerTime);
      item.append(footer);
    }

    allowances.append(item);
  }
}

async function start() {
  await listen<Summary>('usage-summary', event => render(event.payload));
  await listen<WidgetSettings>('widget-settings-changed', event => applySettings(event.payload));
  await listen('status-bar-leave', () => setHovered(false));
  await listen<boolean>('status-bar-hover', event => setHovered(event.payload));
  await listen<string>('status-bar-position-changed', event => {
    const pos = event.payload as WidgetSettings['position'];
    if (pos && ['top', 'bottom', 'left', 'right'].includes(pos)) {
      if (currentSettings.position !== pos) {
        currentSettings.position = pos;
        saveSettings(currentSettings);
        applySettings(currentSettings);
      }
    }
  });
  window.addEventListener('storage', () => applySettings(loadSettings()));
  render(await invoke<Summary>('get_usage_summary'));
}

void start().catch(() => {
  allowances.textContent = 'Status unavailable · right-click to open';
});
