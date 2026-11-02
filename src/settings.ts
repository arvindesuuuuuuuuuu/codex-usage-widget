export type WidgetSettings = {
  normalOpacity: number; // 20 - 100
  hoverOpacity: number;  // 5 - 100
  draggable: boolean;
  clickToOpen: boolean;
  clickThrough: boolean;
};

export const DEFAULT_SETTINGS: WidgetSettings = {
  normalOpacity: 95,
  hoverOpacity: 25,
  draggable: false,
  clickToOpen: false,
  clickThrough: false,
};

export function loadSettings(): WidgetSettings {
  try {
    const raw = localStorage.getItem('widget-settings');
    if (raw) {
      const parsed = JSON.parse(raw);
      return {
        normalOpacity: typeof parsed.normalOpacity === 'number'
          ? Math.max(20, Math.min(100, Math.round(parsed.normalOpacity)))
          : DEFAULT_SETTINGS.normalOpacity,
        hoverOpacity: typeof parsed.hoverOpacity === 'number'
          ? Math.max(5, Math.min(100, Math.round(parsed.hoverOpacity)))
          : DEFAULT_SETTINGS.hoverOpacity,
        draggable: typeof parsed.draggable === 'boolean'
          ? parsed.draggable
          : DEFAULT_SETTINGS.draggable,
        clickToOpen: typeof parsed.clickToOpen === 'boolean'
          ? parsed.clickToOpen
          : DEFAULT_SETTINGS.clickToOpen,
        clickThrough: typeof parsed.clickThrough === 'boolean'
          ? parsed.clickThrough
          : DEFAULT_SETTINGS.clickThrough,
      };
    }
  } catch {}
  return { ...DEFAULT_SETTINGS };
}

export function saveSettings(settings: WidgetSettings) {
  try {
    localStorage.setItem('widget-settings', JSON.stringify(settings));
  } catch {}
}
