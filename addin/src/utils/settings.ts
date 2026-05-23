// Persists user settings via Office.js RoamingSettings (synced across devices).

const KEY_ENGINE_URL = "engineUrl";
const KEY_MODEL_ID = "lastModelId";

export interface AddinSettings {
  engineUrl: string;
  lastModelId: string | null;
}

const DEFAULTS: AddinSettings = {
  engineUrl: "http://127.0.0.1:3000",
  lastModelId: null,
};

function settings(): Office.RoamingSettings {
  return Office.context.roamingSettings;
}

export function loadSettings(): AddinSettings {
  return {
    engineUrl: (settings().get(KEY_ENGINE_URL) as string | undefined) ?? DEFAULTS.engineUrl,
    lastModelId: (settings().get(KEY_MODEL_ID) as string | null | undefined) ?? null,
  };
}

export function saveSettings(s: Partial<AddinSettings>): Promise<void> {
  if (s.engineUrl !== undefined) settings().set(KEY_ENGINE_URL, s.engineUrl);
  if (s.lastModelId !== undefined) settings().set(KEY_MODEL_ID, s.lastModelId);

  return new Promise((resolve, reject) => {
    settings().saveAsync((result) => {
      if (result.status === Office.AsyncResultStatus.Failed) {
        reject(new Error(result.error?.message ?? "Failed to save settings"));
      } else {
        resolve();
      }
    });
  });
}
