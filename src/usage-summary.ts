export type Summary = {
  revision: number;
  provider: string;
  state: 'loading' | 'ready' | 'error';
  message: string;
  windows: { label: string; leftPercent: number }[];
};

// Preserve order when several UI changes occur while the native module loads.
let publishing = Promise.resolve();
export function publishSummary(provider: 'codex' | 'cursor', state: Summary['state'], windows: Summary['windows'], message: string) {
  if (!('__TAURI_INTERNALS__' in window)) return;
  const summary = { provider: provider === 'cursor' ? 'Cursor' : 'Codex', state, windows: windows.slice(0, 8).map(({ label, leftPercent }) => ({ label, leftPercent })), message: message.slice(0, 4096) };
  publishing = publishing.then(async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('publish_usage_summary', { summary });
  }).catch(() => undefined);
}
