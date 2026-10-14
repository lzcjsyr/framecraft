import { useEffect, useRef, useState } from "react";
import {
  Check,
  FolderOpen,
  KeyRound,
  LoaderCircle,
  RefreshCw,
  Sparkles,
  TriangleAlert,
} from "lucide-react";
import { call, date, mediaUrl, seconds } from "./api";
import { t } from "./i18n";
import type { AdcStatus, GenerationSettings, Lang, Workspace } from "./types";

export const providerNames = [
  "BYTEDANCE_TTS_API_KEY",
  "VOLCENGINE_API_KEY",
  "GOOGLE_CLOUD_API_KEY",
] as const;
export type ProviderStatus = Record<string, boolean>;
function AdcLoginCheck({
  lang,
  project,
  busy,
}: {
  lang: Lang;
  project: string;
  busy: boolean;
}) {
  const tr = (k: Parameters<typeof t>[1]) => t(lang, k);
  const [result, setResult] = useState<AdcStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const revision = useRef(0);
  useEffect(() => {
    revision.current += 1;
    setResult(null);
    setChecking(false);
    return () => {
      revision.current += 1;
    };
  }, [project]);
  async function verify() {
    const request = ++revision.current;
    setChecking(true);
    setResult(null);
    try {
      const next = await call<AdcStatus>("adc_status", {
        googleProject: project,
      });
      if (request === revision.current) setResult(next);
    } catch (error) {
      if (request === revision.current)
        setResult({
          loggedIn: false,
          project: null,
          checked: Date.now() / 1000,
          expiresAt: null,
          error: String(error),
        });
    } finally {
      if (request === revision.current) setChecking(false);
    }
  }
  return (
    <div className="cli-login adc-login">
      <div
        className={`cli-login-status ${result?.loggedIn ? "verified" : ""}`}
        role="status"
        aria-live="polite"
      >
        {checking ? (
          <LoaderCircle size={18} className="spin" />
        ) : result?.loggedIn ? (
          <Check size={18} />
        ) : result ? (
          <TriangleAlert size={18} />
        ) : (
          <KeyRound size={18} />
        )}
        <div>
          <strong>
            {tr(
              checking
                ? "checkingAdc"
                : result
                  ? result.loggedIn
                    ? "adcSignedIn"
                    : "adcCheckFailed"
                  : "adcNotChecked",
            )}
          </strong>
          {result?.project ? <small>{result.project}</small> : null}
        </div>
      </div>
      <button
        className="secondary small"
        disabled={busy || checking}
        onClick={verify}
      >
        {checking ? (
          <LoaderCircle size={15} className="spin" />
        ) : (
          <RefreshCw size={15} />
        )}
        {tr("verifyAdc")}
      </button>
      <p className="cli-login-note">{tr("adcCheckNote")}</p>
      {result?.error ? (
        <p className="cli-login-error" role="alert">
          {result.error}
        </p>
      ) : null}
      {result?.loggedIn && !result.project ? (
        <p className="cli-login-note">{tr("adcNoProject")}</p>
      ) : null}
      {result ? (
        <div className="cli-login-details">
          <small>
            {tr("checkedAt")} · {date(result.checked, lang)}
          </small>
          {result.expiresAt ? (
            <small>
              {tr("expiresAt")} · {date(result.expiresAt, lang)}
            </small>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
export function ProviderFields({
  lang,
  value,
  update,
  keys,
  setKeys,
  status,
  busy,
  importKeys,
}: {
  lang: Lang;
  value: GenerationSettings;
  update: (s: GenerationSettings) => void;
  keys: Record<string, string>;
  setKeys: (s: Record<string, string>) => void;
  status: ProviderStatus;
  busy: boolean;
  importKeys: () => void;
}) {
  const tr = (k: Parameters<typeof t>[1]) => t(lang, k);
  const change = <K extends keyof GenerationSettings>(
    k: K,
    v: GenerationSettings[K],
  ) => update({ ...value, [k]: v });
  const keyInput = (name: string) => (
    <label>
      {tr("apiKey")}
      <input
        disabled={busy}
        type="password"
        autoComplete="new-password"
        value={keys[name] || ""}
        aria-label={name}
        placeholder={tr("keyPlaceholder")}
        onChange={(e) => setKeys({ ...keys, [name]: e.target.value })}
      />
      <small className={status[name] ? "key-present" : ""}>
        {tr(status[name] ? "providerKeySaved" : "providerKeyMissing")}
      </small>
    </label>
  );
  return (
    <section className="provider-settings">
      <div className="section-title">
        <span className="section-number">03</span>
        <h2>{tr("mediaProviders")}</h2>
      </div>
      <p className="tool-note">{tr("mediaProviderNote")}</p>
      <button className="secondary small" disabled={busy} onClick={importKeys}>
        <KeyRound size={15} />
        {tr("importProviderKeys")}
      </button>
      <div className="provider-grid">
        <div className="provider-panel">
          <h3>{tr("voiceApi")}</h3>
          {keyInput("BYTEDANCE_TTS_API_KEY")}
          <label>
            {tr("voiceId")}
            <input
              disabled={busy}
              value={value.voice}
              onChange={(e) => change("voice", e.target.value)}
            />
            <small>{tr("voiceNote")}</small>
          </label>
          <label>
            {tr("voiceModel")}
            <select
              disabled={busy}
              value={value.voiceModel}
              onChange={(e) => change("voiceModel", e.target.value)}
            >
              <option>seed-tts-2.0-standard</option>
              <option>seed-tts-2.0-expressive</option>
            </select>
          </label>
          <label>
            {tr("speechRate")} · {value.speechRate}
            <input
              disabled={busy}
              type="range"
              min={-50}
              max={100}
              step={1}
              value={value.speechRate}
              onChange={(e) => change("speechRate", Number(e.target.value))}
            />
          </label>
          <label>
            {tr("emotion")}
            <select
              disabled={busy}
              value={value.emotion}
              onChange={(e) => change("emotion", e.target.value)}
            >
              {(["neutral", "happy", "sad"] as const).map((v) => (
                <option key={v} value={v}>
                  {tr(v)}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="provider-panel">
          <h3>{tr("imageApi")}</h3>
          <label>
            {tr("imageProvider")}
            <select
              disabled={busy}
              value={value.imageProvider}
              onChange={(e) =>
                update({
                  ...value,
                  imageProvider: e.target.value,
                  imageModel:
                    e.target.value === "doubao"
                      ? "doubao-seedream-5-0-pro-260628"
                      : "gemini-3.1-flash-image",
                })
              }
            >
              <option value="doubao">Volcengine · Seedream</option>
              <option value="google">Google · Vertex AI (API key)</option>
              <option value="google_adc">Google · Cloud login (ADC)</option>
            </select>
          </label>
          {value.imageProvider === "google_adc" ? (
            <>
              <label>
                {tr("googleProject")}
                <input
                  disabled={busy}
                  value={value.googleProject}
                  onChange={(e) => change("googleProject", e.target.value)}
                />
                <small>{tr("googleProjectNote")}</small>
              </label>
              <AdcLoginCheck
                lang={lang}
                project={value.googleProject}
                busy={busy}
              />
            </>
          ) : (
            keyInput(
              value.imageProvider === "doubao"
                ? "VOLCENGINE_API_KEY"
                : "GOOGLE_CLOUD_API_KEY",
            )
          )}
          <label>
            {tr("imageModel")}
            <input
              disabled={busy}
              value={value.imageModel}
              list="image-models"
              onChange={(e) => change("imageModel", e.target.value)}
            />
            <datalist id="image-models">
              {(value.imageProvider === "doubao"
                ? [
                    "doubao-seedream-5-0-pro-260628",
                    "doubao-seedream-5-0-260128",
                    "doubao-seedream-5-0-flash-260915",
                  ]
                : ["gemini-3.1-flash-image", "gemini-nano-banana-2.1"]
              ).map((m) => (
                <option key={m} value={m} />
              ))}
            </datalist>
          </label>
          <label>
            {tr("imageSize")}
            <select
              disabled={busy}
              value={value.imageSize}
              onChange={(e) => change("imageSize", e.target.value)}
            >
              <option value="2048x2048">{tr("square")} · 1:1</option>
              <option value="1536x2560">
                {tr("portrait")} ·{" "}
                {value.imageProvider !== "doubao" ? "9:16" : "3:5"}
              </option>
              <option value="2560x1536">
                {tr("landscape")} ·{" "}
                {value.imageProvider !== "doubao" ? "16:9" : "5:3"}
              </option>
            </select>
          </label>
        </div>
      </div>
    </section>
  );
}

export function AssetGenerator({
  lang,
  w,
  settings,
  locked,
  generate,
  configure,
}: {
  lang: Lang;
  w: Workspace;
  settings: GenerationSettings;
  locked: boolean;
  generate: (kind: string, prompt: string) => void;
  configure: () => void;
}) {
  const tr = (k: Parameters<typeof t>[1]) => t(lang, k);
  const [kind, setKind] = useState<"voice" | "image">("voice");
  const [prompts, setPrompts] = useState({ voice: "", image: "" });
  const [status, setStatus] = useState<ProviderStatus>({});
  useEffect(() => {
    let live = true;
    call<ProviderStatus>("provider_status")
      .then((v) => {
        if (live) setStatus(v);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  const script = w.artifacts.findLast((a) => a.kind === "script");
  const scriptPrompt = (script?.data.scenes || script?.data.beats || [])
    .map((b) =>
      kind === "voice" ? b.spoken || b.script || "" : b.visual || "",
    )
    .filter(Boolean)
    .join("\n\n");
  const keyName =
    kind === "voice"
      ? providerNames[0]
      : settings.imageProvider === "doubao"
        ? providerNames[1]
        : settings.imageProvider === "google_adc"
          ? "GOOGLE_ADC"
          : providerNames[2];
  const running = w.runs.some(
    (r) => r.status === "running" && r.stage === kind,
  );
  return (
    <section className="asset-generator">
      <div className="generator-heading">
        <div>
          <h2>{tr("generateAssets")}</h2>
          <p>{tr("generateAssetsNote")}</p>
        </div>
        <button className="text-button" onClick={configure}>
          {tr("configureProviders")}
        </button>
      </div>
      <div
        className="generator-tabs"
        role="group"
        aria-label={tr("generateAssets")}
      >
        {(["voice", "image"] as const).map((v) => (
          <button
            key={v}
            aria-pressed={kind === v}
            className={kind === v ? "active" : ""}
            onClick={() => setKind(v)}
          >
            {tr(v)}
          </button>
        ))}
      </div>
      <label>
        {tr(kind === "voice" ? "narration" : "imagePrompt")}
        <textarea
          rows={4}
          disabled={locked}
          maxLength={8000}
          value={prompts[kind]}
          onChange={(e) => setPrompts({ ...prompts, [kind]: e.target.value })}
        />
      </label>
      <div className="generator-actions">
        <span>
          {kind === "voice"
            ? `${settings.voice || tr("voiceIdRequired")} · ${settings.voiceModel}`
            : settings.imageModel}{" "}
          ·{" "}
          {tr(
            keyName === "GOOGLE_ADC"
              ? status[keyName]
                ? "loginFound"
                : "loginNeeded"
              : status[keyName]
                ? "providerKeySaved"
                : "providerKeyMissing",
          )}
        </span>
        {scriptPrompt ? (
          <button
            className="text-button"
            disabled={locked}
            onClick={() =>
              setPrompts({ ...prompts, [kind]: scriptPrompt.slice(0, 8000) })
            }
          >
            {tr("useScript")}
          </button>
        ) : null}
        <button
          className="primary small"
          disabled={
            locked ||
            !status[keyName] ||
            !prompts[kind].trim() ||
            (kind === "voice" && !settings.voice.trim())
          }
          onClick={() => generate(kind, prompts[kind])}
        >
          {running ? (
            <LoaderCircle size={16} className="spin" />
          ) : (
            <Sparkles size={16} />
          )}
          {tr("generate")}
        </button>
      </div>
    </section>
  );
}

export function GeneratedAssets({
  lang,
  w,
  reveal,
}: {
  lang: Lang;
  w: Workspace;
  reveal: (p: string) => void;
}) {
  const tr = (k: Parameters<typeof t>[1]) => t(lang, k);
  const assets = w.artifacts
    .filter((a) => a.kind === "voice" || a.kind === "image")
    .toReversed();
  if (!assets.length) return null;
  return (
    <section className="generated-section">
      <h2>{tr("generatedAssets")}</h2>
      <div className="generated-grid">
        {assets.map((a) => (
          <article key={a.id}>
            {a.kind === "image" ? (
              <img src={mediaUrl(a.path)} alt={a.title} loading="lazy" />
            ) : (
              <audio controls preload="metadata" src={mediaUrl(a.path)} />
            )}
            <div>
              <span className="output-type">
                {tr(a.kind === "voice" ? "voice" : "image")}
                {a.kind === "voice"
                  ? ` · ${seconds(a.data.duration || 0)}`
                  : ""}
              </span>
              <h3>{a.title}</h3>
              <time>{date(a.created, lang)}</time>
              <button className="text-button" onClick={() => reveal(a.path)}>
                <FolderOpen size={15} />
                {tr("showFile")}
              </button>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
