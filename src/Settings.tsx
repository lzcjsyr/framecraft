import { useState } from "react";
import { Check, LoaderCircle } from "lucide-react";
import { call, describe, native } from "./api";
import { Modal } from "./Modal";
import type { AdcStatus, Diagnostics, Settings } from "./types";
const geminiModels = [
  ["gemini-3.1-flash-image", "Gemini 3.1 Flash Image"],
  ["gemini-nano-banana-2.1", "Nano Banana 2.1"],
] as const;
export function SettingsView({
  settings,
  onClose,
  onSave,
}: {
  settings: Settings;
  onClose: () => void;
  onSave: (settings: Settings, keys: Record<string, string>) => Promise<void>;
}) {
  const [draft, setDraft] = useState<Settings>({
    ...settings,
    generation: {
      ...settings.generation,
      imageProvider:
        settings.generation.imageProvider === "google"
          ? "google_adc"
          : settings.generation.imageProvider,
      // The project now always comes from the ADC login's quota project.
      googleProject: "",
    },
  });
  const [keys, setKeys] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [status, setStatus] = useState<Record<string, boolean>>({});
  const [tools, setTools] = useState<Diagnostics>();
  const [adc, setAdc] = useState<AdcStatus>();
  const [verified, setVerified] = useState(false);
  const generation = draft.generation;
  const update = (key: keyof typeof generation, value: string) =>
    setDraft({ ...draft, generation: { ...generation, [key]: value } });
  async function verify() {
    setBusy(true);
    setMessage("");
    try {
      const results = await Promise.allSettled([
        call<Record<string, boolean>>("check_provider_keys", {
          imageProvider: generation.imageProvider,
        }),
        call<Diagnostics>("diagnostics"),
        call<AdcStatus>("adc_status", {
          googleProject: generation.googleProject,
        }),
      ]);
      const [providers, diagnostics, google] = results;
      if (providers.status === "fulfilled") setStatus(providers.value);
      if (diagnostics.status === "fulfilled") {
        setTools(diagnostics.value);
      }
      if (google.status === "fulfilled") setAdc(google.value);
      setVerified(true);
      const errors = results
        .filter((r) => r.status === "rejected")
        .map((r) => describe((r as PromiseRejectedResult).reason));
      if (errors.length) setMessage(errors.join("\n"));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal title="Settings" busy={busy} onClose={onClose} wide>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          setMessage("");
          try {
            await onSave(draft, keys);
            onClose();
          } catch (error) {
            setMessage(describe(error));
          } finally {
            setBusy(false);
          }
        }}
      >
        <fieldset disabled={busy}>
          <div className="settings-section">
            <h3>Speech · MiMo</h3>
            <p>
              MiMo provides narration and transcription. Choose the agent's
              connection in the Claude Code panel.
            </p>
            <label>
              MiMo API key{" "}
              <input
                type="password"
                autoComplete="off"
                placeholder={
                  status.MIMO_API_KEY
                    ? "Saved locally"
                    : "Enter key to save, or leave blank to keep saved key"
                }
                value={keys.MIMO_API_KEY || ""}
                onChange={(e) =>
                  setKeys({ ...keys, MIMO_API_KEY: e.target.value })
                }
              />
            </label>
            <div className="voice-field">
              <label>
                Narration voice
                <select
                  value={generation.voice}
                  onChange={(e) => update("voice", e.target.value)}
                >
                  {[
                    "mimo_default",
                    "冰糖",
                    "茉莉",
                    "苏打",
                    "白桦",
                    "Mia",
                    "Chloe",
                    "Milo",
                    "Dean",
                  ].map((voice) => (
                    <option key={voice}>{voice}</option>
                  ))}
                </select>
              </label>
            </div>
          </div>
          <div className="settings-section">
            <h3>Image generation</h3>
            <label>
              Provider
              <select
                value={
                  generation.imageProvider === "google"
                    ? "google_adc"
                    : generation.imageProvider
                }
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    generation: {
                      ...generation,
                      imageProvider: e.target.value,
                      imageModel:
                        e.target.value === "doubao"
                          ? "doubao-seedream-5-0-pro-260628"
                          : "gemini-3.1-flash-image",
                    },
                  })
                }
              >
                <option value="google_adc">
                  Google · Application Default Credentials
                </option>
                <option value="doubao">Seedream · Volcengine</option>
              </select>
            </label>
            {generation.imageProvider === "doubao" ? (
              <label>
                Seedream API key
                <input
                  type="password"
                  autoComplete="off"
                  placeholder={
                    status.VOLCENGINE_API_KEY
                      ? "Saved locally"
                      : "Leave blank to keep saved key"
                  }
                  value={keys.VOLCENGINE_API_KEY || ""}
                  onChange={(e) =>
                    setKeys({ ...keys, VOLCENGINE_API_KEY: e.target.value })
                  }
                />
              </label>
            ) : (
              <p className="help-text">
                Uses your local{" "}
                <code>gcloud auth application-default login</code> and its
                quota project.
              </p>
            )}
            <label>
              Image model
              {generation.imageProvider === "doubao" ? (
                <input
                  required
                  value={generation.imageModel}
                  onChange={(e) => update("imageModel", e.target.value)}
                />
              ) : (
                <select
                  value={generation.imageModel}
                  onChange={(e) => update("imageModel", e.target.value)}
                >
                  {geminiModels.map(([id, name]) => (
                    <option key={id} value={id}>
                      {name}
                    </option>
                  ))}
                  {/* Keep a previously saved model visible until it is changed. */}
                  {geminiModels.some(([id]) => id === generation.imageModel) ? null : (
                    <option value={generation.imageModel}>
                      {generation.imageModel}
                    </option>
                  )}
                </select>
              )}
            </label>
          </div>
          <details className="advanced-settings">
            <summary>Executable paths</summary>
            <p>Leave blank to detect installed tools.</p>
            {(
              [
                ["claudePath", "Claude Code"],
                ["ffmpegPath", "FFmpeg"],
                ["ffprobePath", "FFprobe"],
              ] as const
            ).map(([key, name]) => (
              <label key={key}>
                {name}
                <input
                  value={draft[key]}
                  placeholder="Auto-detect"
                  onChange={(e) =>
                    setDraft({ ...draft, [key]: e.target.value })
                  }
                />
              </label>
            ))}
          </details>
        </fieldset>
        {verified ? (
          <div className="connection-results" aria-live="polite">
            <p>
              {status.MIMO_API_KEY ? <Check size={14} /> : null}MiMo key:{" "}
              {status.MIMO_API_KEY ? "saved" : "missing"}
            </p>
            <p>Claude Code: {tools?.claude || "not found"}</p>
            <p>
              FFmpeg: {tools?.ffmpeg ? "installed" : "not found"} · FFprobe:{" "}
              {tools?.ffprobe ? "installed" : "not found"}
            </p>
            <p>
              Google ADC:{" "}
              {adc?.loggedIn
                ? `connected${adc.project ? ` · ${adc.project}` : ""}`
                : adc?.error || "unavailable"}
            </p>
            <small>
              Keys are checked for presence. Creating an asset tests provider
              access.
            </small>
          </div>
        ) : null}
        {message ? (
          <p role="alert" className="form-error">
            {message}
          </p>
        ) : null}
        <div className="modal-actions">
          <button
            type="button"
            className="outline-button"
            disabled={busy || !native}
            onClick={verify}
          >
            {busy ? <LoaderCircle size={15} className="spin" /> : null}Check
            connections
          </button>
          <button className="primary-button" disabled={busy || !native}>
            Save settings
          </button>
        </div>
      </form>
    </Modal>
  );
}
