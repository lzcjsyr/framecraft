import { useEffect, useRef, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { call, describe, native } from "./api";
import type { CliStatus, Settings } from "./types";

export function AgentConnection({ settings, disabled, onSave, onPending }: {
  settings: Settings;
  disabled: boolean;
  onSave: (settings: Settings, keys: Record<string, string>) => Promise<void>;
  onPending: (pending: boolean) => void;
}) {
  const [draft, setDraft] = useState(settings);
  const [key, setKey] = useState("");
  const [savedKeys, setSavedKeys] = useState<Record<string, boolean>>({});
  const [login, setLogin] = useState<CliStatus>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const signature = [settings.mode, settings.baseUrl, settings.model, settings.claudePath].join("\n");
  const committed = useRef(signature);
  useEffect(() => {
    const changed = committed.current !== signature;
    setDraft((previous) => changed ? settings : {
      ...settings, mode: previous.mode, baseUrl: previous.baseUrl, model: previous.model,
    });
    if (changed) setKey("");
    committed.current = signature;
  }, [settings, signature]);
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    call<Record<string, boolean>>("provider_status")
      .then((keys) => { if (!disposed) setSavedKeys(keys); })
      .catch((e) => { if (!disposed) setError(describe(e)); });
    return () => { disposed = true; };
  }, [settings]);
  const local = draft.mode === "local";
  const keyName = draft.mode === "mimo" ? "MIMO_API_KEY" : "AGENT_API_KEY";
  const changed = draft.mode !== settings.mode || draft.model !== settings.model || draft.baseUrl !== settings.baseUrl || !!key;
  useEffect(() => { onPending(changed || busy); }, [changed, busy, onPending]);
  async function checkLogin() {
    setBusy(true); setError("");
    try { setLogin(await call<CliStatus>("cli_status", { claudePath: draft.claudePath })); }
    catch (e) { setError(describe(e)); }
    finally { setBusy(false); }
  }
  return (
    <div className="agent-connection">
      <select aria-label="Agent authentication" disabled={disabled || busy}
        value={local ? "local" : "api"}
        onChange={(e) => {
          setError("");
          setKey("");
          setDraft(e.target.value === "local"
            ? { ...draft, mode: "local", model: "sonnet" }
            : { ...draft, mode: "mimo", model: "mimo-v2.5", baseUrl: "https://api.xiaomimimo.com/anthropic" });
        }}>
        <option value="local">Local Claude login</option>
        <option value="api">Third-party API</option>
      </select>
      <div className="connection-status" aria-live="polite">
        {changed ? <span>Connection changes not saved</span> : local ? (
          <span>{login?.loggedIn ? `Signed in${login.account ? ` · ${login.account}` : ""}` : login?.error || "Login not checked"}</span>
        ) : <span>{draft.mode === "mimo" ? "MiMo" : "Custom provider"} · {savedKeys[keyName] ? "key saved locally" : native ? "key required" : "desktop connection"}</span>}
        {local ? <button className="text-button" disabled={disabled || busy} onClick={() => void checkLogin()}>
          {busy ? <LoaderCircle size={12} className="spin" /> : null}Check login
        </button> : null}
      </div>
      <details className="connection-details" open={changed ? true : undefined}>
        <summary>Connection settings</summary>
        <form onSubmit={async (e) => {
          e.preventDefault(); setBusy(true); setError("");
          try {
            await onSave(draft, key && !local ? { [keyName]: key } : {});
            setKey("");
          } catch (e) { setError(describe(e)); }
          finally { setBusy(false); }
        }}>
          <fieldset disabled={disabled || busy}>
            {!local ? <>
              <label>Provider<select aria-label="Agent API provider" value={draft.mode} onChange={(e) => {
                setKey("");
                setDraft(e.target.value === "mimo"
                  ? { ...draft, mode: "mimo", model: "mimo-v2.5", baseUrl: "https://api.xiaomimimo.com/anthropic" }
                  : { ...draft, mode: "api", model: "sonnet", baseUrl: "https://api.anthropic.com" });
              }}><option value="mimo">Xiaomi MiMo</option><option value="api">Custom Anthropic-compatible API</option></select></label>
              <label>Base URL<input type="url" required value={draft.baseUrl} onChange={(e) => setDraft({ ...draft, baseUrl: e.target.value })} /></label>
              <label>API key<input type="password" autoComplete="off" value={key}
                placeholder={savedKeys[keyName] ? "Saved locally · blank keeps key" : "Saved in local .env during development"}
                onChange={(e) => setKey(e.target.value)} /></label>
            </> : <p className="help-text">Uses the account signed into your installed Claude Code. To sign in, run <code>claude auth login</code> in Terminal, then check login.</p>}
            <label>Model<input required value={draft.model} onChange={(e) => setDraft({ ...draft, model: e.target.value })} /></label>
            <button className="small-button" type="submit">Save connection</button>
          </fieldset>
        </form>
      </details>
      {error ? <p role="alert" className="failure-text">{error}</p> : null}
    </div>
  );
}
