import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowUpRight,
  ArrowRight,
  BookOpen,
  Check,
  Clapperboard,
  FolderOpen,
  Layers,
  LoaderCircle,
  Plus,
  Settings2,
  SlidersHorizontal,
  Sparkles,
  TriangleAlert,
  X,
  History,
  PanelTop,
  Globe,
  Folder,
} from "lucide-react";
import type {
  AppData,
  Artifact,
  Brief,
  Lang,
  Plan,
  Settings,
  Workspace,
} from "./types";
import { call, native, pick } from "./api";
import { t, type Key } from "./i18n";
import {
  BriefView,
  StudyView,
  EditView,
  SkillsView,
  OutputsView,
  RunsView,
} from "./Studio";
import { SettingsView } from "./Settings";
import "./style.css";
type Tab =
  | "brief"
  | "study"
  | "edit"
  | "skills"
  | "outputs"
  | "runs"
  | "settings";
const defaultSettings: Settings = {
  mode: "local",
  baseUrl: "https://api.anthropic.com",
  model: "sonnet",
  claudePath: "",
  ffmpegPath: "",
  ffprobePath: "",
  whisperPath: "",
  generation: {
    voice: "",
    voiceModel: "seed-tts-2.0-standard",
    speechRate: 0,
    emotion: "neutral",
    imageProvider: "doubao",
    imageModel: "doubao-seedream-5-0-pro-260628",
    imageSize: "2048x2048",
    googleProject: "",
  },
};
const tabs: [Tab, typeof BookOpen][] = [
  ["brief", SlidersHorizontal],
  ["study", BookOpen],
  ["edit", Clapperboard],
  ["skills", Layers],
  ["outputs", PanelTop],
  ["runs", History],
];
export default function App() {
  const [lang, setLang] = useState<Lang>(() =>
    localStorage.getItem("framecraft.language") === "zh" ? "zh" : "en",
  );
  const tr = (key: Key) => t(lang, key);
  const [data, setData] = useState<AppData>({
    recents: [],
    settings: defaultSettings,
  });
  const [w, setW] = useState<Workspace | null>(null);
  const workspaceRef = useRef(w);
  workspaceRef.current = w;
  const [brief, setBrief] = useState<Brief | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [tab, setTab] = useState<Tab>("brief");
  const [artifactId, setArtifactId] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [modal, setModal] = useState(false);
  const [workspaceMode, setWorkspaceMode] = useState<"create" | "open">(
    "create",
  );
  const [modalError, setModalError] = useState("");
  const workspaceTrigger = useRef<HTMLButtonElement>(null);
  const workspaceDialog = useRef<HTMLFormElement>(null);
  const busyRef = useRef(false);
  busyRef.current = busy;
  const [name, setName] = useState("");
  const [folder, setFolder] = useState("");
  const [logs, setLogs] = useState<Record<string, string[]>>({});
  const activeRun = w?.runs.find((r) => r.status === "running");
  const locked = busy || !!activeRun;
  const dirty =
    !!w &&
    !!brief &&
    (JSON.stringify(w.brief) !== JSON.stringify(brief) ||
      JSON.stringify(w.selectedSkills) !== JSON.stringify(selected));
  const assign = (next: Workspace) => {
    setW(next);
    setBrief(next.brief);
    setSelected(next.selectedSkills);
  };
  async function refreshData() {
    setData(await call<AppData>("app_data"));
  }
  async function perform(fn: () => Promise<void>) {
    setBusy(true);
    setError("");
    setModalError("");
    try {
      await fn();
    } catch (e) {
      if (modal) setModalError(String(e));
      else setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    if (!modal) return;
    const form = workspaceDialog.current;
    form?.querySelector<HTMLElement>('input, [role="tab"]')?.focus();
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape" && !busyRef.current) {
        event.preventDefault();
        setModal(false);
      }
      if (event.key !== "Tab" || !form) return;
      const controls = Array.from(
        form.querySelectorAll<HTMLElement>(
          "button:not(:disabled), input:not(:disabled)",
        ),
      ).filter((el) => el.offsetParent !== null && el.tabIndex >= 0);
      const first = controls[0],
        last = controls.at(-1);
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    }
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      workspaceTrigger.current?.focus();
    };
  }, [modal]);
  function chooseWorkspaceMode(mode: "create" | "open") {
    setWorkspaceMode(mode);
    setFolder("");
    setModalError("");
  }
  function addWorkspace() {
    setWorkspaceMode("create");
    setName("");
    setFolder("");
    setModalError("");
    setModal(true);
  }
  useEffect(() => {
    if (native) {
      call<AppData>("app_data")
        .then(setData)
        .catch((e) => setError(String(e)));
    }
  }, []);
  useEffect(() => {
    localStorage.setItem("framecraft.language", lang);
    document.documentElement.lang = lang === "en" ? "en" : "zh-CN";
  }, [lang]);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(""), 3500);
    return () => clearTimeout(timer);
  }, [notice]);
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    const unsub: (() => void)[] = [];
    Promise.all([
      listen<{ workspaceId: string; runId: string; line: string }>(
        "run-log",
        (e) => {
          setLogs((old) => ({
            ...old,
            [e.payload.runId]: [
              ...(old[e.payload.runId] || []),
              e.payload.line,
            ].slice(-600),
          }));
        },
      ),
      listen<{ workspaceId: string }>("workspace-updated", async (e) => {
        const id = e.payload.workspaceId;
        try {
          const next = await call<Workspace>("get_workspace", { id });
          if (workspaceRef.current?.id === id) {
            assign(next);
            const last = next.runs.at(-1);
            if (last?.status === "succeeded" && last.artifactId) {
              setArtifactId(last.artifactId);
              setTab(
                last.stage === "plan"
                  ? "edit"
                  : ["render", "voice", "image"].includes(last.stage)
                    ? "outputs"
                    : "study",
              );
            }
          }
          await refreshData();
        } catch (err) {
          setError(String(err));
        }
      }),
    ])
      .then((fns) => {
        if (disposed) fns.forEach((f) => f());
        else unsub.push(...fns);
      })
      .catch((e) => setError(String(e)));
    return () => {
      disposed = true;
      unsub.forEach((f) => f());
    };
  }, []);
  async function persist(): Promise<Workspace> {
    if (!w || !brief) throw new Error("No workspace selected");
    if (!dirty) return w;
    const next = await call<Workspace>("save_brief", {
      id: w.id,
      brief,
      selectedSkills: selected,
    });
    assign(next);
    return next;
  }
  async function switchWorkspace(id: string) {
    await perform(async () => {
      if (dirty && !activeRun) await persist();
      const next = await call<Workspace>("get_workspace", { id });
      assign(next);
      setArtifactId("");
      setTab("brief");
    });
  }
  async function importInput(
    kind: "reference" | "raw" | "transcript" | "skill",
    mediaId?: string,
  ) {
    await perform(async () => {
      const current = await persist();
      const path = await pick(
        kind === "reference"
          ? "video"
          : kind === "raw"
            ? "folder"
            : kind === "transcript"
              ? "transcript"
              : "skill",
      );
      if (!path) return;
      const command =
        kind === "reference"
          ? "import_reference"
          : kind === "raw"
            ? "import_raw_folder"
            : kind === "transcript"
              ? "attach_transcript"
              : "import_skill";
      assign(
        await call<Workspace>(command, {
          id: current.id,
          ...(kind === "raw" ? { folder: path } : { path }),
          ...(mediaId ? { mediaId } : {}),
        }),
      );
      await refreshData();
    });
  }
  async function start(stage: string, planId?: string) {
    await perform(async () => {
      const current = await persist();
      const next = await call<Workspace>("start_run", {
        id: current.id,
        stage,
        artifactId: planId || null,
      });
      assign(next);
      setLogs((old) => ({ ...old, [next.runs.at(-1)!.id]: [] }));
    });
  }
  async function reveal(path: string) {
    await perform(async () => {
      if (w) await call("reveal_path", { id: w.id, path });
    });
  }
  function viewArtifact(a: Artifact) {
    setArtifactId(a.id);
    setTab(
      a.kind === "plan"
        ? "edit"
        : a.kind === "analysis" || a.kind === "script"
          ? "study"
          : "outputs",
    );
  }
  async function savePlan(id: string, plan: Plan) {
    await perform(async () => {
      if (!w) return;
      const next = await call<Workspace>("save_plan", {
        id: w.id,
        artifactId: id,
        plan,
      });
      assign(next);
      setArtifactId(next.artifacts.at(-1)!.id);
      setNotice(tr("savedPlan"));
    });
  }
  const latest = (kind: string) =>
    w?.artifacts.findLast((a) => a.kind === kind);
  const workflow = [
    {
      n: "01",
      title: tr("learn"),
      sub: tr("learnSub"),
      stage: "analyze",
      action: tr("studyAction"),
      done: !!latest("analysis"),
      enabled: !!w?.reference,
    },
    {
      n: "02",
      title: tr("adapt"),
      sub: tr("adaptSub"),
      stage: "adapt",
      action: tr("adaptAction"),
      done: !!latest("script"),
      enabled: !!latest("analysis") && !!brief?.topic.trim(),
    },
    {
      n: "03",
      title: tr("assemble"),
      sub: tr("assembleSub"),
      stage: "plan",
      action: tr("planAction"),
      done: !!latest("plan"),
      enabled:
        !!latest("analysis") &&
        !!w?.media.some((m) => m.kind === "video") &&
        !!brief?.topic.trim(),
    },
    {
      n: "04",
      title: tr("export"),
      sub: tr("exportSub"),
      stage: "render",
      action: tr("reviewPlan"),
      done: !!latest("export"),
      enabled: !!latest("plan"),
    },
  ];
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">
            <span />
            <span />
            <span />
          </div>
          <div>
            <strong>
              framecraft<span>®</span>
            </strong>
            <small>{tr("studio")}</small>
          </div>
        </div>
        <div className="side-heading">
          <span>{tr("workspaces")}</span>
        </div>
        <button
          ref={workspaceTrigger}
          className="workspace-add"
          disabled={busy}
          onClick={addWorkspace}
        >
          <Plus size={16} />
          {tr("addWorkspace")}
        </button>
        <div className="workspace-list">
          {data.recents.map((r) => (
            <button
              key={r.id}
              disabled={busy}
              className={`workspace-item ${w?.id === r.id ? "active" : ""}`}
              onClick={() => switchWorkspace(r.id)}
            >
              <Folder size={17} />
              <span>{r.name}</span>
              {w?.id === r.id ? <span className="current-dot" /> : null}
            </button>
          ))}
          {!data.recents.length ? (
            <div className="no-workspace">{tr("welcomeNote")}</div>
          ) : null}
        </div>
        <div className="side-separator" />
        {w ? (
          <nav>
            {tabs.map(([id, Icon]) => (
              <button
                key={id}
                className={`nav-item ${tab === id ? "active" : ""}`}
                onClick={() => setTab(id)}
              >
                <Icon size={18} />
                {tr(id)}
                {id === "outputs" && w.artifacts.length ? (
                  <span className="nav-count">{w.artifacts.length}</span>
                ) : null}
              </button>
            ))}
          </nav>
        ) : (
          <div className="side-illustration">
            <div />
            <div />
            <div />
            <span>REF → REFRAME</span>
          </div>
        )}
        <div className="side-bottom">
          <button
            className={`nav-item ${tab === "settings" ? "active" : ""}`}
            onClick={() => setTab("settings")}
          >
            <Settings2 size={18} />
            {tr("settings")}
          </button>
          <div className="local-note">
            <span className="current-dot" />
            <div>
              <strong>{tr("localFirst")}</strong>
              <small>{tr("localSub")}</small>
            </div>
          </div>
        </div>
      </aside>
      <main className="main-shell">
        <header className="topbar">
          <div className="breadcrumb">
            <span>{tr("workspace")}</span>
            <span>/</span>
            <strong>
              {tab === "settings" ? tr("settings") : w?.name || "Framecraft"}
            </strong>
          </div>
          <div className="top-actions">
            <button
              className="language-switch"
              onClick={() => setLang(lang === "en" ? "zh" : "en")}
              aria-label="Switch language"
            >
              <Globe size={14} />
              {lang === "en" ? "中文" : "English"}
            </button>
            {w && tab !== "settings" ? (
              <button
                className={`save-button ${dirty ? "dirty" : ""}`}
                disabled={!dirty || locked}
                onClick={() =>
                  perform(async () => {
                    await persist();
                    setNotice(tr("savedBrief"));
                  })
                }
              >
                {busy ? (
                  <LoaderCircle size={14} className="spin" />
                ) : dirty ? (
                  <span className="dirty-dot" />
                ) : (
                  <Check size={14} />
                )}{" "}
                {tr(dirty ? "save" : "saved")}
              </button>
            ) : null}
          </div>
        </header>
        {!native ? (
          <div className="preview-banner">{tr("browserNote")}</div>
        ) : null}
        {error ? (
          <div role="alert" className="error-banner">
            <TriangleAlert size={18} />
            <div>
              <strong>{tr("error")}</strong>
              <p>{error}</p>
            </div>
            <button
              className="icon-btn"
              aria-label={tr("dismiss")}
              onClick={() => setError("")}
            >
              <X size={17} />
            </button>
          </div>
        ) : null}
        {notice ? (
          <div role="status" className="toast">
            <Check size={16} />
            {notice}
          </div>
        ) : null}
        <div
          className={`content-layout ${w && tab !== "settings" ? "with-rail" : ""}`}
        >
          <section className="page-content">
            {tab === "settings" ? (
              <SettingsView
                lang={lang}
                settings={data.settings}
                busy={busy}
                perform={perform}
                onSave={async (settings) => {
                  setData((d) => ({ ...d, settings }));
                  setNotice(tr("savedSettings"));
                }}
              />
            ) : !w ? (
              <div className="welcome">
                <div className="welcome-glyph">
                  <div />
                  <div />
                  <div />
                  <span>
                    <Clapperboard size={48} strokeWidth={1} />
                  </span>
                </div>
                <h1>{tr("welcomeTitle")}</h1>
                <p>{tr("welcomeSub")}</p>
                <p className="welcome-start">
                  {tr(data.recents.length ? "welcomeChoose" : "welcomeStart")}
                </p>
                <div className="welcome-flow">
                  {workflow.map((s, i) => (
                    <div key={s.n}>
                      <span className="step-number">{s.n}</span>
                      <strong>{s.title}</strong>
                      <small>{s.sub}</small>
                      {i < 3 ? <ArrowRight size={16} /> : null}
                    </div>
                  ))}
                </div>
                <div className="welcome-foot">
                  <Folder size={14} />
                  {tr("welcomeNote")}
                </div>
              </div>
            ) : tab === "brief" && brief ? (
              <BriefView
                lang={lang}
                w={w}
                brief={brief}
                setBrief={setBrief}
                locked={locked}
                importInput={importInput}
                reveal={reveal}
                refresh={() =>
                  perform(async () => {
                    const current = await persist();
                    assign(
                      await call<Workspace>("import_raw_folder", {
                        id: current.id,
                        folder: current.rawFolder,
                      }),
                    );
                  })
                }
              />
            ) : tab === "study" ? (
              <StudyView
                lang={lang}
                w={w}
                artifactId={artifactId}
                setArtifactId={setArtifactId}
                locked={locked}
                start={start}
                goBrief={() => setTab("brief")}
              />
            ) : tab === "edit" ? (
              <EditView
                lang={lang}
                w={w}
                artifactId={artifactId}
                setArtifactId={setArtifactId}
                locked={locked}
                start={start}
                savePlan={savePlan}
                goBrief={() => setTab("brief")}
                attach={(id) => importInput("transcript", id)}
              />
            ) : tab === "skills" ? (
              <SkillsView
                lang={lang}
                w={w}
                selected={selected}
                setSelected={setSelected}
                locked={locked}
                importSkill={() => importInput("skill")}
              />
            ) : tab === "outputs" ? (
              <OutputsView
                lang={lang}
                w={w}
                view={viewArtifact}
                settings={data.settings.generation}
                locked={locked}
                generate={(kind, prompt) =>
                  perform(async () => {
                    assign(
                      await call<Workspace>("generate_asset", {
                        id: w.id,
                        kind,
                        prompt,
                      }),
                    );
                  })
                }
                configure={() => setTab("settings")}
                reveal={reveal}
              />
            ) : tab === "runs" ? (
              <RunsView
                lang={lang}
                w={w}
                logs={logs}
                locked={locked}
                start={start}
                view={viewArtifact}
              />
            ) : null}
          </section>
          {w && tab !== "settings" ? (
            <aside className="workflow-rail">
              <div className="rail-heading">
                <span>{tr("workflow")}</span>
                <Sparkles size={15} />
              </div>
              <div className="workflow-steps">
                {workflow.map((s) => (
                  <div
                    key={s.n}
                    className={`workflow-step ${s.done ? "done" : ""} ${activeRun?.stage === s.stage ? "live" : ""}`}
                  >
                    <div className="step-dot">
                      {activeRun?.stage === s.stage ? (
                        <LoaderCircle size={14} className="spin" />
                      ) : s.done ? (
                        <Check size={14} />
                      ) : (
                        s.n
                      )}
                    </div>
                    <div>
                      <strong>{s.title}</strong>
                      <p>{s.sub}</p>
                      {s.stage === "render" ? (
                        <button
                          className="step-action"
                          disabled={locked || !s.enabled}
                          onClick={() => {
                            setTab("edit");
                            setArtifactId(latest("plan")!.id);
                          }}
                        >
                          {s.action}
                          <ArrowUpRight size={14} />
                        </button>
                      ) : (
                        <button
                          className="step-action"
                          disabled={locked || !s.enabled}
                          onClick={() => start(s.stage)}
                        >
                          {s.action}
                          <ArrowRight size={14} />
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </div>
              <div className="agent-panel">
                <div className="agent-header">
                  <span className={activeRun ? "pulse-dot" : "idle-dot"} />
                  <strong>{tr("activity")}</strong>
                  {activeRun ? (
                    <span className="running-label">{tr("running")}</span>
                  ) : null}
                </div>
                <div className="agent-log" aria-live="polite">
                  {activeRun ? (
                    (logs[activeRun.id] || []).slice(-8).map((l, i) => (
                      <p key={i}>
                        <span>›</span>
                        {l}
                      </p>
                    ))
                  ) : w.runs.at(-1) ? (
                    <>
                      <div className={`status ${w.runs.at(-1)!.status}`}>
                        {tr(w.runs.at(-1)!.status as Key)}
                      </div>
                      <p>{w.runs.at(-1)!.message}</p>
                    </>
                  ) : (
                    <p className="empty-log">{tr("activityEmpty")}</p>
                  )}
                </div>
                {activeRun ? (
                  <button
                    className="stop-button"
                    onClick={() =>
                      perform(async () => {
                        await call("cancel_run", { id: w.id });
                      })
                    }
                  >
                    <span />
                    {tr("stop")}
                  </button>
                ) : null}
              </div>
              <button
                className="workspace-location"
                onClick={() => reveal(w.root)}
                title={w.root}
              >
                <FolderOpen size={16} />
                <span>
                  {w.name}
                  <small>{w.root}</small>
                </span>
                <ArrowUpRight size={14} />
              </button>
            </aside>
          ) : null}
        </div>
      </main>
      {modal ? (
        <div
          className="modal-backdrop"
          onClick={() => !busy && setModal(false)}
        >
          <form
            className="modal"
            ref={workspaceDialog}
            role="dialog"
            aria-modal="true"
            aria-labelledby="workspace-dialog-title"
            onClick={(e) => e.stopPropagation()}
            onSubmit={(e) => {
              e.preventDefault();
              perform(async () => {
                if (dirty && !activeRun) await persist();
                assign(
                  workspaceMode === "create"
                    ? await call<Workspace>("create_workspace", {
                        name,
                        folder,
                      })
                    : await call<Workspace>("open_workspace", { folder }),
                );
                setModal(false);
                setName("");
                setFolder("");
                setTab("brief");
                setArtifactId("");
                await refreshData();
              });
            }}
          >
            <div className="modal-heading">
              <div className="modal-icon">
                <FolderOpen size={24} />
              </div>
              <button
                type="button"
                className="icon-btn"
                aria-label={tr("close")}
                disabled={busy}
                onClick={() => setModal(false)}
              >
                <X size={20} />
              </button>
            </div>
            <h2 id="workspace-dialog-title">{tr("addWorkspace")}</h2>
            <div
              className="workspace-modes"
              role="tablist"
              aria-label={tr("workspaceAction")}
            >
              {(["create", "open"] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  role="tab"
                  aria-selected={workspaceMode === mode}
                  aria-controls="workspace-panel"
                  id={`workspace-${mode}-tab`}
                  disabled={busy}
                  tabIndex={workspaceMode === mode ? 0 : -1}
                  onClick={() => chooseWorkspaceMode(mode)}
                  onKeyDown={(e) => {
                    if (
                      !["ArrowLeft", "ArrowRight", "Home", "End"].includes(
                        e.key,
                      )
                    )
                      return;
                    e.preventDefault();
                    const next =
                      e.key === "Home"
                        ? "create"
                        : e.key === "End"
                          ? "open"
                          : mode === "create"
                            ? "open"
                            : "create";
                    chooseWorkspaceMode(next);
                    workspaceDialog.current
                      ?.querySelector<HTMLButtonElement>(
                        `#workspace-${next}-tab`,
                      )
                      ?.focus();
                  }}
                >
                  {tr(mode === "create" ? "createNew" : "openExisting")}
                </button>
              ))}
            </div>
            <div
              id="workspace-panel"
              role="tabpanel"
              aria-labelledby={`workspace-${workspaceMode}-tab`}
            >
              <p className="workspace-mode-note">
                {tr(workspaceMode === "create" ? "createNote" : "openNote")}
              </p>
              {workspaceMode === "create" ? (
                <label>
                  {tr("name")}
                  <input
                    required
                    disabled={busy}
                    maxLength={120}
                    placeholder={tr("namePlaceholder")}
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                  />
                </label>
              ) : null}
              <label>
                {tr("folder")}
                <button
                  type="button"
                  className="folder-picker"
                  disabled={busy}
                  onClick={() =>
                    perform(async () => {
                      const p = await pick("folder");
                      if (p) {
                        setFolder(p);
                        if (workspaceMode === "create" && !name.trim())
                          setName(
                            (
                              p.split(/[\\/]/).filter(Boolean).at(-1) || ""
                            ).slice(0, 120),
                          );
                      }
                    })
                  }
                >
                  <FolderOpen size={18} />
                  <span>{folder || tr("chooseFolder")}</span>
                  <ArrowUpRight size={16} />
                </button>
              </label>
            </div>
            {modalError ? (
              <p className="modal-error" role="alert">
                <TriangleAlert size={16} />
                {modalError}
              </p>
            ) : null}
            <div className="modal-actions">
              <button
                type="button"
                className="secondary"
                disabled={busy}
                onClick={() => setModal(false)}
              >
                {tr("cancel")}
              </button>
              <button
                className="primary"
                disabled={
                  busy ||
                  !folder ||
                  (workspaceMode === "create" && !name.trim())
                }
              >
                {busy ? (
                  <LoaderCircle size={17} className="spin" />
                ) : workspaceMode === "create" ? (
                  <Plus size={17} />
                ) : (
                  <FolderOpen size={17} />
                )}{" "}
                {tr(workspaceMode === "create" ? "create" : "openWorkspace")}
              </button>
            </div>
          </form>
        </div>
      ) : null}
    </div>
  );
}
