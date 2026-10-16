import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ChevronDown,
  Clapperboard,
  FolderOpen,
  LoaderCircle,
  Plus,
  Settings2,
  X,
} from "lucide-react";
import { call, defaultBrief, defaultSettings, describe, native, pick } from "./api";
import type { AppData, Brief, Settings, Workspace } from "./types";
import { Guidance } from "./Guidance";
import { Assets } from "./Assets";
import { Result } from "./Result";
import { TerminalPanel, type TerminalHandle } from "./Terminal";
import { SettingsView } from "./Settings";
import "./style.css";
const draftKey = (id?: string) => `framecraft.draft.v2.${id || "new"}`;
function restoredBrief(w?: Workspace): Brief {
  try {
    const draft = JSON.parse(localStorage.getItem(draftKey(w?.id)) || "null");
    if (
      draft?.revision === (w?.revision || 0) &&
      typeof draft.brief?.topic === "string"
    )
      return { ...(w?.brief || defaultBrief), ...draft.brief };
  } catch {
    /* A corrupt local draft must not prevent opening the workspace. */
  }
  return w?.brief || defaultBrief;
}
export default function App() {
  const [data, setData] = useState<AppData>({
    settings: defaultSettings,
    recents: [],
  });
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [brief, setBrief] = useState<Brief>(() => restoredBrief());
  const [busy, setBusy] = useState(native);
  const [error, setError] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [connectionPending, setConnectionPending] = useState(false);
  const [selectedOutput, setSelectedOutput] = useState("");
  const terminal = useRef<TerminalHandle>(null);
  const [terminalRunning, setTerminalRunning] = useState(false);
  const current = useRef(workspace);
  current.current = workspace;
  const briefRef = useRef(brief);
  briefRef.current = brief;
  const inflight = useRef(false);
  const activeRun = workspace?.runs.find((r) => r.status === "running");
  const locked = busy || !!activeRun;
  function assign(w: Workspace, restore = false) {
    const previous = current.current;
    if (previous?.id === w.id) {
      const before = previous.artifacts.filter((a) => a.kind === "export").at(-1);
      const after = w.artifacts.filter((a) => a.kind === "export").at(-1);
      if (after && after.id !== before?.id) setSelectedOutput(after.id);
    }
    current.current = w;
    setWorkspace(w);
    const next = restore ? restoredBrief(w) : { ...w.brief };
    if (!restore && previous?.id === w.id) {
      // A tool progress event must not replace guidance the user is still editing.
      for (const key of Object.keys(next) as (keyof Brief)[]) {
        if (briefRef.current[key] !== previous.brief[key])
          Object.assign(next, { [key]: briefRef.current[key] });
      }
      if (JSON.stringify(next) !== JSON.stringify(w.brief)) {
        try { localStorage.setItem(draftKey(w.id), JSON.stringify({ brief: next, revision: w.revision })); }
        catch { /* The next successful save still writes the native manifest. */ }
      }
    }
    setBrief(next);
    briefRef.current = next;
  }
  function editBrief(next: Brief) {
    setBrief(next);
    briefRef.current = next;
    try {
      localStorage.setItem(
        draftKey(current.current?.id),
        JSON.stringify({
          brief: next,
          revision: current.current?.revision || 0,
        }),
      );
    } catch {
      /* The native manifest remains the source of saved inputs. */
    }
  }
  async function perform(action: () => Promise<void>) {
    if (inflight.current) return;
    inflight.current = true;
    setBusy(true);
    setError("");
    try {
      await action();
    } catch (e) {
      setError(describe(e));
    } finally {
      inflight.current = false;
      setBusy(false);
    }
  }
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    const stops: (() => void)[] = [];
    call<AppData>("app_data")
      .then(async (app) => {
        if (disposed) return;
        setData(app);
        if (app.recents[0]) {
          const w = await call<Workspace>("get_workspace", {
            id: app.recents[0].id,
          });
          if (!disposed) assign(w, true);
        }
      })
      .catch((e) => {
        if (!disposed) setError(describe(e));
      })
      .finally(() => {
        if (!disposed) setBusy(false);
      });
    const subscribe = async () => {
      for (const stop of await Promise.all([
        listen<{ workspaceId: string }>("workspace-updated", async (e) => {
          try {
            const w = await call<Workspace>("get_workspace", {
              id: e.payload.workspaceId,
            });
            if (!disposed && current.current?.id === w.id) assign(w);
            const app = await call<AppData>("app_data");
            if (!disposed) setData(app);
          } catch (e) {
            if (!disposed) setError(describe(e));
          }
        }),
      ])) {
        if (disposed) stop();
        else stops.push(stop);
      }
    };
    subscribe().catch((e) => setError(describe(e)));
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, []);
  // Dropdown menus close on an outside click or Escape, like native menus.
  useEffect(() => {
    const dismiss = (e: Event) => {
      document
        .querySelectorAll<HTMLDetailsElement>("details.menu[open]")
        .forEach((menu) => {
          if (
            e instanceof KeyboardEvent
              ? e.key === "Escape"
              : !menu.contains(e.target as Node)
          )
            menu.open = false;
        });
    };
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("keydown", dismiss);
    return () => {
      document.removeEventListener("pointerdown", dismiss);
      document.removeEventListener("keydown", dismiss);
    };
  }, []);
  // A completed native run is also recovered when the window regains focus.
  useEffect(() => {
    if (!native) return;
    const refresh = () => {
      const w = current.current;
      if (w?.runs.some((r) => r.status === "running"))
        call<Workspace>("get_workspace", { id: w.id })
          .then(assign)
          .catch((e) => setError(describe(e)));
    };
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, []);
  async function ensureWorkspace(): Promise<Workspace | null> {
    if (current.current) return current.current;
    const w = await call<Workspace>("create_workspace", {
      name: "My video",
      folder: null,
    });
    current.current = w;
    setWorkspace(w);
    setData(await call<AppData>("app_data"));
    return w;
  }
  async function save(): Promise<Workspace | null> {
    const w = await ensureWorkspace();
    if (!w) return null;
    const draft = briefRef.current;
    if (
      draft.duration < 5 ||
      draft.duration > 600 ||
      !Number.isFinite(draft.duration)
    )
      throw new Error("Set a target duration from 5 to 600 seconds.");
    if (JSON.stringify(w.brief) === JSON.stringify(draft)) return w;
    const next = await call<Workspace>("save_brief", {
      id: w.id,
      brief: draft,
      selectedSkills: w.selectedSkills,
    });
    localStorage.removeItem(draftKey(w.id));
    localStorage.removeItem(draftKey());
    assign(next);
    return next;
  }
  async function importGuidance(kind: "reference" | "skill") {
    await perform(async () => {
      const w = await save();
      if (!w) return;
      const [path] = await pick(kind === "reference" ? "video" : "skill");
      if (path)
        assign(
          await call<Workspace>(
            kind === "reference" ? "import_reference" : "import_skill",
            { id: w.id, path },
          ),
        );
    });
  }
  async function chooseAssetsFolder() {
    await perform(async () => {
      const [folder] = await pick("folder");
      if (!folder) return;
      if (current.current) {
        await call("terminal_close", { id: current.current.id });
        setTerminalRunning(false);
      }
      if (current.current) await save();
      const draft = briefRef.current;
      const next = await call<Workspace>("set_assets_folder", {
        id: current.current?.id || null,
        folder,
      });
      assign(next);
      editBrief(draft);
      setData(await call<AppData>("app_data"));
    });
  }
  function removeInput(mediaId: string) {
    void perform(async () => {
      const w = await save();
      if (w)
        assign(await call<Workspace>("remove_input", { id: w.id, mediaId }));
    });
  }
  function changeSkills(ids: string[]) {
    void perform(async () => {
      const w = await save();
      if (w)
        assign(
          await call<Workspace>("save_brief", {
            id: w.id,
            brief: briefRef.current,
            selectedSkills: ids,
          }),
        );
    });
  }
  function changeWorkspace(id: string) {
    void perform(async () => {
      if (current.current) {
        await call("terminal_close", { id: current.current.id });
        setTerminalRunning(false);
        await save();
      }
      assign(await call<Workspace>("get_workspace", { id }), true);
      setSelectedOutput("");
    });
  }
  function openWorkspace(create: boolean) {
    void perform(async () => {
      if (current.current) await save();
      const [folder] = await pick("folder");
      if (!folder) return;
      if (current.current) {
        await call("terminal_close", { id: current.current.id });
        setTerminalRunning(false);
      }
      const w = await call<Workspace>(
        create ? "create_workspace" : "open_workspace",
        create
          ? {
              name:
                folder.split(/[\\/]/).filter(Boolean).at(-1) ||
                "Untitled video",
              folder,
            }
          : { folder },
      );
      assign(w, true);
      setSelectedOutput("");
      setData(await call<AppData>("app_data"));
    });
  }
  async function saveSettings(
    settings: Settings,
    keys: Record<string, string>,
  ) {
    if (current.current) {
      await call("terminal_close", { id: current.current.id });
      setTerminalRunning(false);
    }
    const saved = await call<Settings>("save_settings", {
      settings,
      apiKey: null,
      providerKeys: keys,
    });
    setData((old) => ({ ...old, settings: saved }));
  }
  async function openTerminal(): Promise<boolean> {
    if (connectionPending) return false;
    return await terminal.current?.open() || false;
  }
  useEffect(() => {
    if (!native || !workspace) return;
    void call("terminal_context", { id: workspace.id, selected: selectedOutput }).catch((e) => setError(describe(e)));
  }, [workspace?.id, selectedOutput]);
  // Keep the live goal available to tools used from the terminal.
  useEffect(() => {
    if (!terminalRunning || busy || activeRun || !workspace || JSON.stringify(brief) === JSON.stringify(workspace.brief)) return;
    const timer = window.setTimeout(() => { void save().catch((e) => setError(describe(e))); }, 600);
    return () => window.clearTimeout(timer);
  }, [brief, terminalRunning, busy, activeRun, workspace]);
  const missing = !brief.topic.trim()
    ? workspace?.assetsFolder
      ? "Describe your goal to continue."
      : "Add a goal and an assets folder to continue."
    : !workspace?.assetsFolder
      ? "Choose an assets folder to continue."
      : "";
  return (
    <div className="app-shell">
      <header className="topbar">
        <a className="brand" href="#" onClick={(e) => e.preventDefault()}>
          <span className="brand-mark" aria-hidden="true">
            <Clapperboard size={15} strokeWidth={2.2} />
          </span>
          Framecraft
        </a>
        <details className="workspace-menu menu">
          <summary>
            <span className="workspace-name">
              {workspace?.name || "My first video"}
            </span>
            <ChevronDown size={15} />
          </summary>
          <div className="workspace-options">
            <button
              disabled={locked}
              onClick={(e) => {
                e.currentTarget.closest("details")?.removeAttribute("open");
                openWorkspace(true);
              }}
            >
              <Plus size={16} />
              New workspace
            </button>
            <button
              disabled={locked}
              onClick={(e) => {
                e.currentTarget.closest("details")?.removeAttribute("open");
                openWorkspace(false);
              }}
            >
              <FolderOpen size={16} />
              Open workspace
            </button>
            {data.recents.map((r) => (
              <button
                className={workspace?.id === r.id ? "selected" : ""}
                key={r.id}
                disabled={locked}
                onClick={(e) => {
                  e.currentTarget.closest("details")?.removeAttribute("open");
                  changeWorkspace(r.id);
                }}
              >
                {r.name}
              </button>
            ))}
          </div>
        </details>
        <button
          className="settings-button"
          aria-label="Settings"
          disabled={locked}
          onClick={() => setSettingsOpen(true)}
        >
          <Settings2 size={17} />
          <span>Settings</span>
        </button>
      </header>
      {error ? (
        <div className="error-banner" role="alert">
          <span>{error}</span>
          <button
            className="icon-button"
            aria-label="Dismiss error"
            onClick={() => setError("")}
          >
            <X size={16} />
          </button>
        </div>
      ) : null}
      <main className="studio-layout">
        <div className="composer">
          <Guidance
            brief={brief}
            workspace={workspace}
            disabled={locked}
            onChange={editBrief}
            onImport={importGuidance}
            onRemove={removeInput}
            onSkills={changeSkills}
          />
          <div className="assets-section">
            <Assets
              folder={workspace?.assetsFolder || null}
              disabled={locked}
              narration={brief.narration}
              onNarration={(narration) => editBrief({ ...brief, narration })}
              onChoose={() => void chooseAssetsFolder()}
            />
          </div>
          <div className="create-area">
            {activeRun ? (
              <button
                className="outline-button stop-button"
                onClick={() =>
                  void perform(async () => {
                    await call("cancel_run", { id: workspace!.id });
                  })
                }
                disabled={busy}
              >
                <LoaderCircle size={17} className="spin" />
                Stop generation
              </button>
            ) : (
              <button
                className="primary-button create-button"
                disabled={
                  busy || connectionPending || !brief.topic.trim() || !workspace?.assetsFolder
                }
                onClick={() => void openTerminal()}
              >
                {busy ? <LoaderCircle size={17} className="spin" /> : null}
                {terminalRunning ? "Continue in terminal" : "Open Claude Code"}
              </button>
            )}
            {!activeRun && missing ? (
              <p className="create-hint">{missing}</p>
            ) : null}
          </div>
        </div>
        <div className="result-column">
          <Result
            artifacts={workspace?.artifacts || []}
            aspect={brief.aspect}
            active={!!activeRun}
            selected={selectedOutput}
            onSelect={setSelectedOutput}
            onReveal={(path) =>
              void perform(async () => {
                await call("reveal_path", { id: workspace!.id, path });
              })
            }
          />
          {!native ? (
            <p className="browser-note">
              Desktop preview · Open Framecraft to create videos
            </p>
          ) : null}
        </div>
        <TerminalPanel
          ref={terminal}
          workspace={workspace}
          settings={data.settings}
          disabled={locked}
          onPrepare={save}
          onSaveConnection={saveSettings}
          onConnectionPending={setConnectionPending}
          onRunning={setTerminalRunning}
          onInstalled={async () => { setData(await call<AppData>("app_data")); }}
        />
      </main>
      {settingsOpen ? (
        <SettingsView
          settings={data.settings}
          onSave={saveSettings}
          onClose={() => setSettingsOpen(false)}
        />
      ) : null}
    </div>
  );
}
