import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { Download, LoaderCircle, Square, TerminalSquare } from "lucide-react";
import { AgentConnection } from "./AgentConnection";
import { call, describe, native } from "./api";
import type { Settings, Workspace } from "./types";
import "@xterm/xterm/css/xterm.css";

type Event = { type: "data"; bytes: number[] } | { type: "exit"; code: number };
export interface TerminalHandle { open: () => Promise<boolean> }
interface Props {
  workspace: Workspace | null;
  settings: Settings;
  disabled: boolean;
  onPrepare: () => Promise<Workspace | null>;
  onSaveConnection: (settings: Settings, keys: Record<string, string>) => Promise<void>;
  onConnectionPending: (pending: boolean) => void;
  onRunning: (running: boolean) => void;
  onInstalled: () => Promise<void>;
}
export const TerminalPanel = forwardRef<TerminalHandle, Props>(function TerminalPanel(props, ref) {
  const { workspace, settings, disabled, onSaveConnection, onConnectionPending } = props;
  const id = workspace?.id;
  const host = useRef<HTMLDivElement>(null);
  const term = useRef<XTerm | null>(null);
  const fit = useRef<FitAddon | null>(null);
  const latest = useRef(props); latest.current = props;
  const running = useRef(false);
  const [isRunning, setRunning] = useState(false);
  const [starting, setStarting] = useState(false);
  const [pending, setPending] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [installation, setInstallation] = useState<string | null>();
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const transition = useRef(false);
  const input = useRef(Promise.resolve());
  const generation = useRef(0);
  const exited = useRef(0);
  const [hasOutput, setHasOutput] = useState(false);
  const outputSeen = useRef(false);
  async function detectInstallation() {
    const path = await call<string | null>("claude_installation");
    setInstallation(path);
    return path;
  }
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    const check = () => {
      void call<string | null>("claude_installation").then((path) => {
        if (!disposed) setInstallation(path);
      }).catch((e) => { if (!disposed) { setInstallation(null); setError(describe(e)); } });
    };
    check();
    window.addEventListener("focus", check);
    return () => { disposed = true; window.removeEventListener("focus", check); };
  }, [settings.claudePath]);
  function markRunning(value: boolean) {
    running.current = value; setRunning(value); latest.current.onRunning(value);
  }
  function write(data: string) {
    if (!id || !running.current) return Promise.resolve();
    input.current = input.current.then(() => call<void>("terminal_write", { id, data })).catch((e) => setError(describe(e)));
    return input.current;
  }
  function channel(terminal: XTerm) {
    const version = ++generation.current;
    const output = new Channel<Event>();
    output.onmessage = (event) => {
      if (term.current !== terminal || generation.current !== version) return;
      if (event.type === "data") {
        terminal.write(new Uint8Array(event.bytes));
        if (!outputSeen.current) { outputSeen.current = true; setHasOutput(true); }
      }
      else {
        exited.current = version;
        markRunning(false);
        terminal.write(`\r\n\x1b[90mClaude Code exited (${event.code}). Start again to resume.\x1b[0m\r\n`);
      }
    };
    return output;
  }
  useEffect(() => {
    if (!host.current) return;
    let disposed = false;
    outputSeen.current = false; setHasOutput(false); setError(""); setNotice(""); markRunning(false);
    const terminal = new XTerm({
      cursorBlink: true, fontSize: 12, lineHeight: 1.15,
      fontFamily: 'Menlo, Monaco, "SFMono-Regular", monospace', scrollback: 10000,
      theme: { background: "#0f0f14", foreground: "#e4e2ec", cursor: "#a99bff", cursorAccent: "#0f0f14", selectionBackground: "#8f7dff55" },
      allowProposedApi: false,
    });
    const addon = new FitAddon(); terminal.loadAddon(addon);
    term.current = terminal; fit.current = addon;
    terminal.open(host.current);
    const resize = () => {
      if (disposed || !host.current?.clientWidth || !host.current.clientHeight) return;
      addon.fit();
      if (native && id) void call("terminal_resize", { id, cols: terminal.cols, rows: terminal.rows }).catch(() => {});
    };
    const observer = new ResizeObserver(resize); observer.observe(host.current);
    resize();
    terminal.onData((data) => { void write(data); });
    terminal.attachCustomKeyEventHandler((e) => {
      // Leave normal system copy/paste shortcuts to the webview and xterm.
      if (e.metaKey && ["c", "v"].includes(e.key.toLowerCase())) return false;
      return true;
    });
    if (native && id) {
      void call<boolean>("terminal_attach", { id, channel: channel(terminal) })
        .then((active) => { if (!disposed) markRunning(active); })
        .catch((e) => { if (!disposed) setError(describe(e)); });
    } else markRunning(false);
    return () => {
      disposed = true; observer.disconnect();
      term.current = null; fit.current = null; terminal.dispose();
      generation.current++;
      running.current = false;
    };
  }, [id]);
  useEffect(() => { onConnectionPending(pending || updating || starting); }, [pending, updating, starting, onConnectionPending]);
  async function start() {
    if (transition.current || pending || updating || !native || !id || !term.current) return false;
    transition.current = true; setStarting(true); setError(""); setNotice("");
    try {
      if (!await detectInstallation()) return false;
      const w = await latest.current.onPrepare();
      if (!w || !term.current) return false;
      if (running.current) {
        setNotice("Guidance saved. Continue your conversation in the terminal.");
      } else {
        const terminal = term.current;
        terminal.reset(); fit.current?.fit();
        const created = await call<boolean>("terminal_open", { id: w.id, cols: terminal.cols, rows: terminal.rows, channel: channel(terminal) });
        if (exited.current !== generation.current) markRunning(true);
        if (!created) setNotice("Guidance saved. Continue your conversation in the terminal.");
      }
      term.current?.focus();
      return true;
    } catch (e) { setError(describe(e)); return false; }
    finally { transition.current = false; setStarting(false); }
  }
  useImperativeHandle(ref, () => ({ open: start }));
  async function stop() {
    if (!id) return;
    try { await call("terminal_close", { id }); markRunning(false); }
    catch (e) { setError(describe(e)); }
  }
  async function update() {
    setUpdating(true); setError(""); setNotice("");
    try { setNotice(await call<string>("update_claude")); }
    catch (e) { setError(describe(e)); }
    finally { setUpdating(false); }
  }
  async function install() {
    if (transition.current || updating) return;
    transition.current = true; setUpdating(true); setError(""); setNotice("");
    const progress = new Channel<string>();
    progress.onmessage = (line) => setNotice(line.replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, ""));
    try {
      const result = await call<string>("install_claude", { progress });
      await latest.current.onInstalled();
      await detectInstallation();
      setNotice(result);
    } catch (e) { setError(describe(e)); setNotice(""); await detectInstallation().catch(() => {}); }
    finally { transition.current = false; setUpdating(false); }
  }
  return <aside className="terminal-panel" aria-label="Claude Code terminal">
    <div className="terminal-heading">
      <h2>
        <span className="section-icon" aria-hidden="true"><TerminalSquare size={14} /></span>
        Claude Code
        <span className={isRunning ? "live-badge on" : "live-badge"}>{isRunning ? "Live" : "Idle"}</span>
      </h2>
      {!native || installation ? <div className="terminal-actions">
        <button className="small-button" title={isRunning ? "Stop Claude Code before updating" : "Check and update official Claude Code CLI"}
          disabled={!native || isRunning || starting || updating || disabled} onClick={() => void update()}>
          {updating ? <LoaderCircle size={13} className="spin" /> : <Download size={13} />}Update
        </button>
        {isRunning ? <button className="small-button" onClick={() => void stop()}><Square size={12} />Stop</button>
          : <button className="small-button" disabled={!native || !workspace?.assetsFolder || starting || updating || pending || disabled} onClick={() => void start()}>
            {starting ? <LoaderCircle size={13} className="spin" /> : null}Start
          </button>}
      </div> : null}
    </div>
    {!native || installation ? <AgentConnection settings={settings} disabled={disabled || starting || updating} onSave={onSaveConnection} onPending={setPending} /> : null}
    {notice ? <p className="terminal-notice" role="status">{notice}</p> : null}
    {error ? <p className="terminal-notice failure-text" role="alert">{error}</p> : null}
    <div className="terminal-body">
      <div ref={host} className="terminal-surface" />
      {native && !installation ? <div className="terminal-placeholder terminal-setup">
        <TerminalSquare size={28} />
        <h3>{installation === undefined ? "Checking Claude Code…" : "Install Claude Code"}</h3>
        <p>Claude Code runs your video agent.</p>
        <small>Install it once, then choose local Claude login or a third-party API.</small>
        {installation === null ? <>
          <button disabled={updating || disabled} onClick={() => void install()}>
            {updating ? <LoaderCircle size={14} className="spin" /> : <Download size={14} />}
            {updating ? "Installing…" : "Install Claude Code"}
          </button>
          <button className="text-button" disabled={updating} onClick={() => void detectInstallation().catch((e) => setError(describe(e)))}>Already installed? Check again</button>
          <small>Downloads from claude.ai. Existing installs can be selected in Settings.</small>
        </> : null}
      </div> : !isRunning && !starting && !hasOutput ? <div className="terminal-placeholder">
        <p>{native ? workspace?.assetsFolder ? "Start Claude Code to continue." : "Choose an assets folder to start." : "Open Framecraft to use the local terminal."}</p>
        <small>Ask it to create or edit a video. Your goal and assets are available to the agent.</small>
      </div> : null}
    </div>
    <div className="terminal-footer">
      <span>{native && !installation ? "Claude Code setup" : <>{isRunning ? "Interactive CLI" : "Claude Code CLI"} · <kbd>Esc</kbd> interrupts</>}</span>
      <button className="text-button" disabled={!isRunning} onClick={() => { void write("\x0f"); term.current?.focus(); }}>Details <kbd>Ctrl</kbd><kbd>O</kbd></button>
    </div>
  </aside>;
});
