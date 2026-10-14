import { useEffect, useState } from "react";
import {
  Check,
  KeyRound,
  LoaderCircle,
  RefreshCw,
  Save,
  Terminal,
  TriangleAlert,
} from "lucide-react";
import type { CliStatus, Diagnostics, Lang, Settings } from "./types";
import { call, date } from "./api";
import { t, type Key } from "./i18n";
import {
  ProviderFields,
  providerNames,
  type ProviderStatus,
} from "./Generation";
import { PageHeading } from "./Studio";
export function SettingsView({
  lang,
  settings,
  busy,
  perform,
  onSave,
}: {
  lang: Lang;
  settings: Settings;
  busy: boolean;
  perform: (fn: () => Promise<void>) => Promise<void>;
  onSave: (s: Settings) => Promise<void>;
}) {
  const tr = (k: Key) => t(lang, k);
  const [draft, setDraft] = useState(settings);
  const [key, setKey] = useState("");
  const [providerKeys, setProviderKeys] = useState<Record<string, string>>({});
  const [providerStatus, setProviderStatus] = useState<ProviderStatus>({});
  const [credentialError, setCredentialError] = useState(false);
  const [importNote, setImportNote] = useState("");
  useEffect(() => {
    let live = true;
    call<ProviderStatus>("provider_status")
      .then((v) => {
        if (live) setProviderStatus(v);
      })
      .catch(() => {
        if (live) setCredentialError(true);
      });
    return () => {
      live = false;
    };
  }, []);
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [checking, setChecking] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [cli, setCli] = useState<CliStatus | null>(null);
  useEffect(() => setCli(null), [draft.claudePath]);
  useEffect(() => setDraft(settings), [settings]);
  function update(k: keyof Settings, v: string) {
    setDraft((old) => ({ ...old, [k]: v }));
  }
  const paths: [
    "claudePath" | "ffmpegPath" | "ffprobePath" | "whisperPath",
    Key,
  ][] = [
    ["claudePath", "claudePath"],
    ["ffmpegPath", "ffmpegPath"],
    ["ffprobePath", "ffprobePath"],
    ["whisperPath", "whisperPath"],
  ];
  async function save() {
    const saved = await call<Settings>("save_settings", {
      settings: draft,
      apiKey: key || null,
      providerKeys,
    });
    setKey("");
    setProviderKeys({});
    setProviderStatus(await call<ProviderStatus>("provider_status"));
    await onSave(saved);
  }
  return (
    <>
      <PageHeading title={tr("agentTitle")} sub={tr("agentSub")} />
      <div className="settings-form">
        <div className="auth-modes">
          {(["local", "api"] as const).map((mode) => (
            <button
              key={mode}
              className={draft.mode === mode ? "active" : ""}
              disabled={busy}
              onClick={() => update("mode", mode)}
            >
              {mode === "local" ? (
                <Terminal size={24} />
              ) : (
                <KeyRound size={24} />
              )}
              <span>
                <strong>
                  {tr(mode === "local" ? "localMode" : "apiMode")}
                </strong>
                <small>
                  {tr(mode === "local" ? "localModeSub" : "apiModeSub")}
                </small>
              </span>
              <span className="radio-indicator">
                {draft.mode === mode ? <Check size={12} /> : null}
              </span>
            </button>
          ))}
        </div>
        <div className="settings-fields">
          {draft.mode === "api" ? (
            <>
              <label>
                {tr("baseUrl")}
                <input
                  disabled={busy}
                  type="url"
                  value={draft.baseUrl}
                  onChange={(e) => update("baseUrl", e.target.value)}
                />
              </label>
              <label>
                {tr("apiKey")}
                <input
                  aria-label={tr("apiKey")}
                  disabled={busy}
                  type="password"
                  autoComplete="new-password"
                  placeholder={tr("keyPlaceholder")}
                  value={key}
                  onChange={(e) => setKey(e.target.value)}
                />
                <small>{tr("keyNote")}</small>
              </label>
            </>
          ) : (
            <div className="cli-login">
              <div className="cli-login-row">
                <div
                  className={`cli-login-status ${cli?.loggedIn ? "verified" : ""}`}
                  role="status"
                  aria-live="polite"
                >
                  {verifying ? (
                    <LoaderCircle size={20} className="spin" />
                  ) : cli?.loggedIn ? (
                    <Check size={20} />
                  ) : cli ? (
                    <TriangleAlert size={20} />
                  ) : (
                    <Terminal size={20} />
                  )}
                  <div>
                    <strong>
                      {tr(
                        verifying
                          ? "checkingCli"
                          : cli
                            ? cli.loggedIn
                              ? "signedIn"
                              : "signedOut"
                            : "cliNotChecked",
                      )}
                    </strong>
                    {cli?.loggedIn && cli.account ? (
                      <small>{cli.account}</small>
                    ) : null}
                  </div>
                </div>
                <button
                  className="secondary"
                  disabled={busy}
                  onClick={() =>
                    perform(async () => {
                      setVerifying(true);
                      setCli(null);
                      try {
                        setCli(
                          await call<CliStatus>("cli_status", {
                            claudePath: draft.claudePath,
                          }),
                        );
                      } finally {
                        setVerifying(false);
                      }
                    })
                  }
                >
                  {verifying ? (
                    <LoaderCircle size={15} className="spin" />
                  ) : (
                    <RefreshCw size={15} />
                  )}
                  {tr("verifyCli")}
                </button>
              </div>
              <p className="cli-login-note">{tr("cliCheckNote")}</p>
              {cli?.error ? (
                <p className="cli-login-error" role="alert">
                  {cli.error}
                </p>
              ) : null}
              {cli ? (
                <div className="cli-login-details">
                  <span>{cli.version}</span>
                  {cli.path ? <code>{cli.path}</code> : null}
                  <small>
                    {tr("checkedAt")} · {date(cli.checked, lang)}
                  </small>
                </div>
              ) : null}
            </div>
          )}
          <label>
            {tr("model")}
            <input
              disabled={busy}
              value={draft.model}
              onChange={(e) => update("model", e.target.value)}
              placeholder={
                draft.mode === "local" ? "sonnet" : "Provider model ID"
              }
            />
          </label>
          {draft.mode === "api" ? (
            <p className="settings-note">{tr("apiHelp")}</p>
          ) : null}
        </div>
        <div className="section-title">
          <span className="section-number">02</span>
          <h2>{tr("tools")}</h2>
        </div>
        <p className="tool-note">{tr("toolNote")}</p>
        <div className="tool-fields">
          {paths.map(([key, label]) => (
            <label key={key}>
              {tr(label)}
              <input
                disabled={busy}
                value={draft[key]}
                onChange={(e) => update(key, e.target.value)}
                placeholder={tr("autodetect")}
              />
            </label>
          ))}
        </div>
        {credentialError ? (
          <div className="credential-error" role="alert">
            <p>{tr("credentialError")}</p>
            <button
              className="secondary small"
              disabled={busy}
              onClick={() =>
                perform(async () => {
                  setProviderStatus(
                    await call<ProviderStatus>("unlock_provider_keys"),
                  );
                  setCredentialError(false);
                })
              }
            >
              <KeyRound size={15} />
              {tr("unlockProviderKeys")}
            </button>
          </div>
        ) : null}
        <ProviderFields
          lang={lang}
          value={draft.generation}
          update={(generation) => setDraft((old) => ({ ...old, generation }))}
          keys={providerKeys}
          setKeys={setProviderKeys}
          status={providerStatus}
          busy={busy}
          importKeys={() =>
            perform(async () => {
              const status = await call<ProviderStatus>("import_provider_keys");
              setProviderStatus(status);
              setImportNote(
                tr(
                  providerNames.some((name) => status[name])
                    ? "providerKeySaved"
                    : "noProviderKeys",
                ),
              );
            })
          }
        />
        {importNote ? (
          <p role="status" className="tool-note">
            {importNote}
          </p>
        ) : null}
        <div className="settings-actions">
          <button
            className="primary"
            disabled={busy}
            onClick={() => perform(save)}
          >
            <Save size={17} />
            {tr("save")}
          </button>
          <button
            className="secondary"
            disabled={busy}
            onClick={() =>
              perform(async () => {
                setChecking(true);
                try {
                  await save();
                  setDiagnostics(await call<Diagnostics>("diagnostics"));
                } finally {
                  setChecking(false);
                }
              })
            }
          >
            {checking ? (
              <LoaderCircle size={16} className="spin" />
            ) : (
              <RefreshCw size={16} />
            )}{" "}
            {tr("check")}
          </button>
        </div>
        {diagnostics ? (
          <div className="diagnostics">
            {diagnostics.keySaved ? (
              <p className="diagnostic-key">
                <KeyRound size={13} />
                {tr("keySaved")}
              </p>
            ) : null}
            {(["claude", "ffmpeg", "ffprobe", "whisper"] as const).map(
              (tool) => (
                <div className="diagnostic-tool" key={tool}>
                  <span
                    className={diagnostics[tool] ? "ok-dot" : "missing-dot"}
                  />
                  <strong>
                    {tool === "claude"
                      ? "Claude Code"
                      : tool === "whisper"
                        ? "Whisper"
                        : tool.toUpperCase()}
                  </strong>
                  <code>{diagnostics[tool] || tr("missing")}</code>
                </div>
              ),
            )}
            {diagnostics.errors.map((error, i) => (
              <p className="diagnostic-error" key={i}>
                {error}
              </p>
            ))}
          </div>
        ) : null}
      </div>
    </>
  );
}
