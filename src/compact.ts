import './compact.css';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Summary } from './usage-summary';

document.querySelector('#app')!.innerHTML = `
  <button id="restore" class="compact" aria-label="Restore full usage window" title="Click to restore the full widget">
    <span class="provider" id="compact-provider">Usage</span>
    <span class="allowances" id="compact-allowances" aria-live="polite">Connecting…</span>
    <span class="restore-icon" aria-hidden="true">↗</span>
  </button>`;

const button = document.querySelector<HTMLButtonElement>('#restore')!;
const label = document.querySelector<HTMLElement>('#compact-provider')!;
const allowances = document.querySelector<HTMLElement>('#compact-allowances')!;
let revision = -1;

function render(summary: Summary) {
  if (summary.revision < revision) return;
  revision = summary.revision;
  label.textContent = summary.provider || 'Usage';
  button.dataset.state = summary.state;
  button.title = `${summary.message || 'Usage'} — click to restore`;
  allowances.replaceChildren();
  if (summary.state !== 'ready' || !summary.windows.length) {
    const message = document.createElement('span');
    message.className = 'message';
    message.textContent = summary.state === 'error' ? 'Usage unavailable · open for details' : (summary.message || 'Connecting…');
    allowances.append(message);
    return;
  }
  for (const window of summary.windows.slice(0, 2)) {
    const left = Math.max(0, Math.min(100, window.leftPercent));
    const item = document.createElement('span');
    item.className = 'allowance';
    const text = document.createElement('span');
    text.className = 'allowance-text';
    const name = document.createElement('span');
    name.textContent = window.label;
    const percent = document.createElement('strong');
    percent.textContent = `${left.toFixed(left % 1 ? 1 : 0)}% left`;
    text.append(name, percent);
    const track = document.createElement('span');
    track.className = 'mini-track';
    const fill = document.createElement('span');
    fill.className = left <= 10 ? 'mini-fill low' : 'mini-fill';
    fill.style.width = `${left}%`;
    track.append(fill);
    item.append(text, track);
    allowances.append(item);
  }
}

button.addEventListener('click', async () => {
  try { await invoke('restore_usage_window'); }
  catch { allowances.textContent = 'Could not restore · try the tray icon'; }
});

async function start() {
  await listen<Summary>('usage-summary', event => render(event.payload));
  render(await invoke<Summary>('get_usage_summary'));
}
void start().catch(() => { allowances.textContent = 'Status unavailable · click to open'; });
