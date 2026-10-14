import { AssetGenerator, GeneratedAssets } from "./Generation";
import { useEffect, useRef, useState } from "react";
import {
  ArrowDown,
  ArrowRight,
  ArrowUp,
  ArrowUpRight,
  BookOpen,
  Check,
  Clapperboard,
  FileText,
  Film,
  FolderOpen,
  Image,
  Layers,
  Music,
  Plus,
  RefreshCw,
  Save,
  Sparkles,
  Trash2,
  TriangleAlert,
  X,
} from "lucide-react";
import type { Artifact, Brief, Clip, Lang, Plan, Workspace } from "./types";
import { date, mediaUrl, seconds } from "./api";
import { t, type Key } from "./i18n";
export function PageHeading({
  title,
  sub,
  action,
}: {
  title: string;
  sub: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="page-heading">
      <div>
        <h1>{title}</h1>
        <p>{sub}</p>
      </div>
      {action}
    </div>
  );
}
function Empty({
  icon: Icon,
  title,
  sub,
  action,
}: {
  icon: typeof Film;
  title: string;
  sub?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="empty-state">
      <div className="empty-icon">
        <Icon size={35} strokeWidth={1.3} />
      </div>
      <h2>{title}</h2>
      {sub ? <p>{sub}</p> : null}
      {action}
    </div>
  );
}
export function BriefView({
  lang,
  w,
  brief,
  setBrief,
  locked,
  importInput,
  reveal,
  refresh,
}: {
  lang: Lang;
  w: Workspace;
  brief: Brief;
  setBrief: (b: Brief) => void;
  locked: boolean;
  importInput: (
    kind: "reference" | "raw" | "transcript",
    mediaId?: string,
  ) => void;
  reveal: (path: string) => void;
  refresh: () => void;
}) {
  const tr = (k: Key) => t(lang, k);
  const update = (key: keyof Brief, value: string | number) =>
    setBrief({ ...brief, [key]: value });
  return (
    <>
      <PageHeading title={tr("briefTitle")} sub={tr("briefSub")} />
      <div className="section-title">
        <span className="section-number">01</span>
        <h2>{tr("brief")}</h2>
      </div>
      <div className="brief-fields">
        <label className="wide topic-label">
          {tr("topic")}
          <input
            value={brief.topic}
            disabled={locked}
            maxLength={4000}
            placeholder={tr("topicPlaceholder")}
            onChange={(e) => update("topic", e.target.value)}
          />
        </label>
        <label className="wide">
          {tr("direction")}
          <textarea
            rows={3}
            value={brief.direction}
            disabled={locked}
            maxLength={8000}
            placeholder={tr("directionPlaceholder")}
            onChange={(e) => update("direction", e.target.value)}
          />
        </label>
        <label>
          {tr("audience")}
          <input
            value={brief.audience}
            disabled={locked}
            placeholder={tr("audiencePlaceholder")}
            onChange={(e) => update("audience", e.target.value)}
          />
        </label>
        <label>
          {tr("platform")}
          <select
            value={brief.platform}
            disabled={locked}
            onChange={(e) => update("platform", e.target.value)}
          >
            {[
              "Douyin / TikTok",
              "Xiaohongshu",
              "WeChat Channels",
              "YouTube",
              "Instagram Reels",
              "Other",
            ].map((p) => (
              <option key={p}>{p}</option>
            ))}
          </select>
        </label>
        <div className="brief-bottom">
          <label>
            {tr("duration")}
            <div className="input-suffix">
              <input
                type="number"
                min={5}
                max={600}
                value={brief.duration}
                disabled={locked}
                onChange={(e) => update("duration", Number(e.target.value))}
              />
              <span>{tr("seconds")}</span>
            </div>
          </label>
          <label>
            {tr("aspect")}
            <select
              value={brief.aspect}
              disabled={locked}
              onChange={(e) => update("aspect", e.target.value)}
            >
              <option value="9:16">9:16 · {tr("portrait")}</option>
              <option value="16:9">16:9 · {tr("landscape")}</option>
              <option value="1:1">1:1 · {tr("square")}</option>
            </select>
          </label>
          <label>
            {tr("outputLanguage")}
            <select
              value={brief.language}
              disabled={locked}
              onChange={(e) => update("language", e.target.value)}
            >
              <option>简体中文</option>
              <option>English</option>
              <option>繁體中文</option>
            </select>
          </label>
        </div>
      </div>
      <div className="section-title source-title">
        <span className="section-number">02</span>
        <h2>{tr("inputs")}</h2>
      </div>
      <div className="input-sources">
        <div className={`source-box ${w.reference ? "loaded" : ""}`}>
          <div className="source-icon">
            <Film size={22} />
          </div>
          <div className="source-copy">
            <h3>{tr("reference")}</h3>
            <p>{w.reference?.name || tr("referenceSub")}</p>
            {w.reference ? (
              <small>
                {w.reference.duration > 0
                  ? `${seconds(w.reference.duration)} · ${w.reference.width} × ${w.reference.height}`
                  : w.reference.path}
              </small>
            ) : null}
          </div>
          <button
            className="secondary small"
            disabled={locked}
            onClick={() => importInput("reference")}
          >
            {tr(w.reference ? "change" : "chooseVideo")}
            <Plus size={14} />
          </button>
          {w.reference ? (
            <div className="source-detail">
              <span
                className={
                  w.reference.transcript
                    ? "transcript-ok"
                    : "transcript-missing"
                }
              >
                {w.reference.transcript ? (
                  <Check size={13} />
                ) : (
                  <TriangleAlert size={13} />
                )}{" "}
                {tr(
                  w.reference.transcript ? "transcriptReady" : "noTranscript",
                )}
              </span>
              <button
                className="text-button"
                disabled={locked}
                onClick={() => importInput("transcript", w.reference!.id)}
              >
                <FileText size={14} />
                {tr("transcript")}
              </button>
            </div>
          ) : null}
        </div>
        <div className={`source-box ${w.rawFolder ? "loaded" : ""}`}>
          <div className="source-icon">
            <FolderOpen size={22} />
          </div>
          <div className="source-copy">
            <h3>{tr("raw")}</h3>
            <p>
              {w.rawFolder ? w.rawFolder.split(/[\\/]/).at(-1) : tr("rawSub")}
            </p>
            {w.rawFolder ? (
              <small>
                {w.media.length} {tr("files")} ·{" "}
                {w.media.filter((m) => m.kind === "video").length}{" "}
                {tr("videos")}
              </small>
            ) : null}
          </div>
          <button
            className="secondary small"
            disabled={locked}
            onClick={() => importInput("raw")}
          >
            {tr(w.rawFolder ? "change" : "chooseRaw")}
            <Plus size={14} />
          </button>
          {w.rawFolder ? (
            <div className="source-detail">
              <button
                className="text-button"
                disabled={locked}
                onClick={refresh}
              >
                <RefreshCw size={13} />
                {tr("refresh")}
              </button>
              <button
                className="text-button"
                onClick={() => reveal(w.rawFolder!)}
              >
                {tr("showFolder")}
                <ArrowUpRight size={14} />
              </button>
            </div>
          ) : null}
        </div>
      </div>
      <div className="brief-footer">
        <Layers size={15} />
        <span>
          {w.selectedSkills.length} {tr("skills")} {tr("selected")}
        </span>
        <span className="footer-separator" />
        <span>Claude Code</span>
        <span className="footer-separator" />
        <span>{tr("localFirst")}</span>
      </div>
    </>
  );
}
function Warnings({ lang, items }: { lang: Lang; items?: string[] }) {
  return items?.length ? (
    <div className="warnings">
      <div>
        <TriangleAlert size={16} />
        <strong>{t(lang, "warnings")}</strong>
      </div>
      <ul>
        {items.map((x, i) => (
          <li key={i}>{x}</li>
        ))}
      </ul>
    </div>
  ) : null;
}
export function StudyView({
  lang,
  w,
  artifactId,
  setArtifactId,
  locked,
  start,
  goBrief,
}: {
  lang: Lang;
  w: Workspace;
  artifactId: string;
  setArtifactId: (id: string) => void;
  locked: boolean;
  start: (stage: string) => void;
  goBrief: () => void;
}) {
  const tr = (k: Key) => t(lang, k);
  const results = w.artifacts.filter(
    (a) => a.kind === "analysis" || a.kind === "script",
  );
  const selected = results.find((a) => a.id === artifactId) || results.at(-1);
  const [section, setSection] = useState("analysis");
  const active =
    selected?.kind === section
      ? selected
      : results.findLast((a) => a.kind === section) || selected;
  useEffect(() => {
    if (selected) setSection(selected.kind);
  }, [selected?.id]);
  if (!results.length)
    return (
      <>
        <PageHeading title={tr("study")} sub={tr("learnSub")} />
        <Empty
          icon={BookOpen}
          title={tr("emptyStudy")}
          sub={tr("emptyStudySub")}
          action={
            <button className="primary" onClick={goBrief}>
              {tr("goBrief")}
              <ArrowRight size={16} />
            </button>
          }
        />
      </>
    );
  const d = active!.data;
  const rows = d.beats || d.scenes || [];
  return (
    <>
      <PageHeading
        title={tr("study")}
        sub={w.reference?.name || tr("learnSub")}
        action={
          <button
            className="secondary"
            disabled={locked || !w.brief.topic.trim()}
            onClick={() => start("adapt")}
          >
            <Sparkles size={16} />
            {tr("adaptAction")}
          </button>
        }
      />
      <div className="result-toolbar">
        <div className="segmented">
          <button
            className={section === "analysis" ? "active" : ""}
            onClick={() => {
              setSection("analysis");
              const a = results.findLast((a) => a.kind === "analysis");
              if (a) setArtifactId(a.id);
            }}
          >
            {tr("analysis")}
          </button>
          <button
            className={section === "script" ? "active" : ""}
            disabled={!results.some((a) => a.kind === "script")}
            onClick={() => {
              setSection("script");
              const a = results.findLast((a) => a.kind === "script");
              if (a) setArtifactId(a.id);
            }}
          >
            {tr("script")}
          </button>
        </div>
        <select
          aria-label={tr("version")}
          value={active?.id}
          onChange={(e) => setArtifactId(e.target.value)}
        >
          {results
            .filter((a) => a.kind === active?.kind)
            .toReversed()
            .map((a, i) => (
              <option key={a.id} value={a.id}>
                {tr("version")}{" "}
                {results.filter((a) => a.kind === active?.kind).length - i} ·{" "}
                {date(a.created, lang)}
              </option>
            ))}
        </select>
      </div>
      <div className="result-intro">
        <h2>{d.title}</h2>
        <p>{d.summary}</p>
      </div>
      <Warnings lang={lang} items={d.warnings} />
      {active?.kind === "analysis" ? (
        <div className="analysis-facts">
          {(
            [
              ["hook", d.hook],
              ["visualStyle", d.visualStyle],
              ["pacing", d.pacing],
              ["sound", d.audio],
            ] as [Key, string | undefined][]
          ).map(([key, value]) => (
            <div key={key}>
              <h3>{tr(key)}</h3>
              <p>{value}</p>
            </div>
          ))}
        </div>
      ) : (
        <div className="script-hook">
          <strong>{tr("hook")}</strong>
          <p>{d.hook}</p>
        </div>
      )}
      <div className="section-title">
        <span className="section-number">
          {rows.length.toString().padStart(2, "0")}
        </span>
        <h2>{tr("structure")}</h2>
      </div>
      <div className="beat-list">
        {rows.map((b, i) => (
          <article className="beat" key={i}>
            <div className="beat-index">
              <span>{(i + 1).toString().padStart(2, "0")}</span>
              <time>
                {seconds(b.start)}
                <br />— {seconds(b.end)}
              </time>
            </div>
            <div className="beat-content">
              <h3>{b.purpose}</h3>
              <dl>
                <div>
                  <dt>{tr("visual")}</dt>
                  <dd>{b.visual}</dd>
                </div>
                <div>
                  <dt>{tr("spoken")}</dt>
                  <dd>{b.spoken || b.script || "—"}</dd>
                </div>
                {b.onScreen ? (
                  <div>
                    <dt>{tr("onScreen")}</dt>
                    <dd>{b.onScreen}</dd>
                  </div>
                ) : null}
                {b.audio ? (
                  <div>
                    <dt>{tr("sound")}</dt>
                    <dd>{b.audio}</dd>
                  </div>
                ) : null}
                {b.editing ? (
                  <div>
                    <dt>{tr("edit")}</dt>
                    <dd>{b.editing}</dd>
                  </div>
                ) : null}
              </dl>
              {b.evidence ? (
                <p className="evidence">
                  <span>{tr("evidence")}</span>
                  {b.evidence}
                </p>
              ) : null}
            </div>
          </article>
        ))}
      </div>
      {d.transferableRules?.length ? (
        <div className="transfer-rules">
          <h3>{tr("rules")}</h3>
          {d.transferableRules.map((rule, i) => (
            <p key={i}>
              <Check size={15} />
              {rule}
            </p>
          ))}
        </div>
      ) : null}
    </>
  );
}
export function EditView({
  lang,
  w,
  artifactId,
  setArtifactId,
  locked,
  start,
  savePlan,
  goBrief,
  attach,
}: {
  lang: Lang;
  w: Workspace;
  artifactId: string;
  setArtifactId: (id: string) => void;
  locked: boolean;
  start: (stage: string, id?: string) => void;
  savePlan: (id: string, plan: Plan) => void;
  goBrief: () => void;
  attach: (id: string) => void;
}) {
  const tr = (k: Key) => t(lang, k);
  const plans = w.artifacts.filter((a) => a.kind === "plan");
  const a = plans.find((a) => a.id === artifactId) || plans.at(-1);
  const [clips, setClips] = useState<Clip[]>([]);
  const [active, setActive] = useState(0);
  const [sourceId, setSourceId] = useState("");
  const video = useRef<HTMLVideoElement>(null);
  useEffect(() => {
    setClips(a?.data.clips?.map((c) => ({ ...c })) || []);
    setActive(0);
    setSourceId(
      a?.data.clips?.[0]?.sourceId ||
        w.media.find((m) => m.kind === "video")?.id ||
        "",
    );
  }, [a?.id]);
  const chosen = w.media.find((m) => m.id === sourceId);
  const activeClip = clips[active];
  const changed = JSON.stringify(clips) !== JSON.stringify(a?.data.clips || []);
  const total = clips.reduce((n, c) => n + c.end - c.start, 0);
  const stale = a?.data._inputRevision !== w.revision;
  const invalid = clips.some(
    (c) =>
      !Number.isFinite(c.start) ||
      !Number.isFinite(c.end) ||
      c.end - c.start < 0.15 ||
      c.start < 0 ||
      c.end > (w.media.find((m) => m.id === c.sourceId)?.duration || 0) + 0.025,
  );
  function chooseClip(i: number) {
    setActive(i);
    setSourceId(clips[i].sourceId);
    if (video.current && sourceId === clips[i].sourceId) {
      video.current.currentTime = clips[i].start;
      video.current.pause();
    }
  }
  function update(i: number, key: keyof Clip, value: string | number) {
    setClips((old) =>
      old.map((c, j) => (j === i ? { ...c, [key]: value } : c)),
    );
  }
  function move(i: number, by: number) {
    setClips((old) => {
      const next = [...old];
      [next[i], next[i + by]] = [next[i + by], next[i]];
      return next;
    });
    setActive(i + by);
  }
  if (!a && !w.media.length)
    return (
      <>
        <PageHeading title={tr("edit")} sub={tr("assembleSub")} />
        <Empty
          icon={Clapperboard}
          title={tr("emptyEdit")}
          sub={tr("emptyEditSub")}
          action={
            <button className="primary" onClick={goBrief}>
              {tr("goBrief")}
              <ArrowRight size={16} />
            </button>
          }
        />
      </>
    );
  return (
    <>
      <PageHeading
        title={tr("edit")}
        sub={tr("assembleSub")}
        action={
          <button
            className="secondary"
            disabled={
              locked ||
              !w.reference ||
              !w.artifacts.some((a) => a.kind === "analysis")
            }
            onClick={() => start("plan")}
          >
            <Sparkles size={16} />
            {tr("planAction")}
          </button>
        }
      />
      <div className="edit-top">
        <div className="source-preview">
          <div className="preview-label">
            <Film size={14} />
            {tr("preview")}
            <span>{chosen?.name}</span>
          </div>
          {chosen?.kind === "video" ? (
            <video
              key={chosen.path}
              ref={video}
              src={mediaUrl(chosen.path)}
              controls
              preload="metadata"
              onLoadedMetadata={() => {
                if (video.current && activeClip?.sourceId === chosen.id)
                  video.current.currentTime = activeClip.start;
              }}
              onTimeUpdate={() => {
                if (
                  video.current &&
                  activeClip?.sourceId === chosen.id &&
                  video.current.currentTime >= activeClip.end &&
                  !video.current.paused
                ) {
                  video.current.pause();
                }
              }}
            />
          ) : (
            <div className="preview-empty">
              <Film size={30} strokeWidth={1} />
              <span>{tr("selectClip")}</span>
            </div>
          )}
        </div>
        <div className="media-library">
          <h3>
            {tr("sourceLibrary")} <span>{w.media.length}</span>
          </h3>
          {w.media.map((m) => (
            <div
              key={m.id}
              className={`media-row ${sourceId === m.id ? "active" : ""}`}
            >
              <button onClick={() => setSourceId(m.id)}>
                {m.kind === "video" ? (
                  <Film size={15} />
                ) : m.kind === "audio" ? (
                  <Music size={15} />
                ) : (
                  <Image size={15} />
                )}
                <span>
                  <strong>{m.name}</strong>
                  <small>
                    {m.duration
                      ? seconds(m.duration)
                      : `${(m.bytes / 1024 / 1024).toFixed(1)} MB`}
                    {m.transcript ? " · TXT" : ""}
                  </small>
                </span>
              </button>
              {m.kind !== "image" ? (
                <button
                  className="icon-btn"
                  disabled={locked}
                  title={tr("transcript")}
                  onClick={() => attach(m.id)}
                >
                  <FileText size={14} />
                </button>
              ) : null}
            </div>
          ))}
        </div>
      </div>
      {a ? (
        <>
          <div className="result-toolbar">
            <h2>{tr("timeline")}</h2>
            <select
              value={a.id}
              aria-label={tr("version")}
              onChange={(e) => setArtifactId(e.target.value)}
            >
              {plans.toReversed().map((p, i) => (
                <option key={p.id} value={p.id}>
                  {tr("version")} {plans.length - i} · {date(p.created, lang)}
                </option>
              ))}
            </select>
          </div>
          <div className="plan-intro">
            <h3>{a.data.title}</h3>
            <p>{a.data.summary}</p>
          </div>
          <Warnings
            lang={lang}
            items={
              stale
                ? [tr("stale"), ...(a.data.warnings || [])]
                : a.data.warnings
            }
          />
          <div className="timeline-ribbon">
            {clips.map((c, i) => (
              <button
                key={i}
                style={{ flex: Math.max(0.2, c.end - c.start) }}
                className={active === i ? "active" : ""}
                onClick={() => chooseClip(i)}
                title={c.beat}
              >
                <span>{(i + 1).toString().padStart(2, "0")}</span>
                <small>{(c.end - c.start).toFixed(1)}s</small>
              </button>
            ))}
          </div>
          <div className="timeline-list">
            {clips.map((c, i) => (
              <article
                className={`clip-editor ${active === i ? "active" : ""}`}
                key={i}
              >
                <div className="clip-top">
                  <button className="clip-number" onClick={() => chooseClip(i)}>
                    {(i + 1).toString().padStart(2, "0")}
                  </button>
                  <strong>{c.beat}</strong>
                  <span>{(c.end - c.start).toFixed(1)}s</span>
                  <div className="clip-actions">
                    <button
                      className="icon-btn"
                      title={tr("moveUp")}
                      disabled={locked || i === 0}
                      onClick={() => move(i, -1)}
                    >
                      <ArrowUp size={14} />
                    </button>
                    <button
                      className="icon-btn"
                      title={tr("moveDown")}
                      disabled={locked || i === clips.length - 1}
                      onClick={() => move(i, 1)}
                    >
                      <ArrowDown size={14} />
                    </button>
                    <button
                      className="icon-btn"
                      title={tr("removeClip")}
                      disabled={locked || clips.length === 1}
                      onClick={() => {
                        setClips((old) => old.filter((_, j) => j !== i));
                        setActive(0);
                      }}
                    >
                      <Trash2 size={14} />
                    </button>
                  </div>
                </div>
                <div className="clip-fields">
                  <label>
                    {tr("source")}
                    <select
                      value={c.sourceId}
                      disabled={locked}
                      onChange={(e) => update(i, "sourceId", e.target.value)}
                    >
                      {w.media
                        .filter((m) => m.kind === "video")
                        .map((m) => (
                          <option value={m.id} key={m.id}>
                            {m.name}
                          </option>
                        ))}
                    </select>
                  </label>
                  <label>
                    {tr("in")}
                    <input
                      type="number"
                      min={0}
                      step={0.1}
                      value={c.start}
                      disabled={locked}
                      onChange={(e) =>
                        update(i, "start", Number(e.target.value))
                      }
                    />
                  </label>
                  <label>
                    {tr("out")}
                    <input
                      type="number"
                      min={0}
                      step={0.1}
                      value={c.end}
                      disabled={locked}
                      onChange={(e) => update(i, "end", Number(e.target.value))}
                    />
                  </label>
                  <label className="clip-caption">
                    {tr("caption")}
                    <input
                      value={c.caption}
                      maxLength={300}
                      disabled={locked}
                      onChange={(e) => update(i, "caption", e.target.value)}
                    />
                  </label>
                </div>
                <p className="clip-reason">{c.reason}</p>
              </article>
            ))}
          </div>
          <div className="render-bar">
            <div>
              <strong>
                {tr("total")} · {seconds(Math.max(0, total))}
              </strong>
              <small>{tr("sidecar")}</small>
            </div>
            {changed ? (
              <button
                className="secondary"
                disabled={locked || invalid || stale}
                onClick={() =>
                  savePlan(a.id, {
                    title: a.data.title || a.title,
                    summary: a.data.summary || "",
                    warnings: a.data.warnings || [],
                    clips,
                  })
                }
              >
                <Save size={16} />
                {tr("savePlan")}
              </button>
            ) : null}
            <button
              className="primary"
              disabled={locked || changed || invalid || stale}
              onClick={() => start("render", a.id)}
            >
              <Clapperboard size={17} />
              {tr("renderAction")}
            </button>
          </div>
        </>
      ) : (
        <Empty icon={Layers} title={tr("emptyEdit")} sub={tr("emptyEditSub")} />
      )}
    </>
  );
}
export function SkillsView({
  lang,
  w,
  selected,
  setSelected,
  locked,
  importSkill,
}: {
  lang: Lang;
  w: Workspace;
  selected: string[];
  setSelected: (s: string[]) => void;
  locked: boolean;
  importSkill: () => void;
}) {
  const tr = (k: Key) => t(lang, k);
  const [detail, setDetail] = useState("");
  const names: Record<string, [Key, Key]> = {
    "reference-study": ["refStudy", "refDesc"],
    "story-adaptation": ["storyAdaptation", "adaptDesc"],
    "footage-editing": ["footageEditing", "editDesc"],
    "social-pacing": ["socialPacing", "pacingDesc"],
  };
  return (
    <>
      <PageHeading
        title={tr("skillsTitle")}
        sub={tr("skillsSub")}
        action={
          <button className="secondary" disabled={locked} onClick={importSkill}>
            <Plus size={16} />
            {tr("importSkill")}
          </button>
        }
      />
      <div className="skill-list">
        {w.skills.map((s, i) => (
          <article
            className={`skill-row ${selected.includes(s.id) ? "selected" : ""}`}
            key={s.id}
          >
            <span className="skill-index">
              {(i + 1).toString().padStart(2, "0")}
            </span>
            <label className="skill-selection">
              <input
                type="checkbox"
                disabled={locked}
                checked={selected.includes(s.id)}
                onChange={() =>
                  setSelected(
                    selected.includes(s.id)
                      ? selected.filter((id) => id !== s.id)
                      : [...selected, s.id],
                  )
                }
              />
              <div>
                <span className="skill-kind">
                  {tr(s.origin === "Built-in" ? "builtin" : "workspaceSkill")}
                </span>
                <h3>{names[s.id] ? tr(names[s.id][0]) : s.name}</h3>
                <p>{names[s.id] ? tr(names[s.id][1]) : s.description}</p>
              </div>
            </label>
            <button
              className="text-button"
              onClick={() => setDetail(detail === s.id ? "" : s.id)}
            >
              {detail === s.id ? <X size={14} /> : <FileText size={14} />}{" "}
              {tr("viewInstructions")}
            </button>
            {detail === s.id ? (
              <pre className="skill-instructions">{s.instructions}</pre>
            ) : null}
          </article>
        ))}
      </div>
      <p className="skill-note">
        <Layers size={16} />
        {tr("skillsNote")}
      </p>
    </>
  );
}
export function OutputsView({
  lang,
  w,
  view,
  reveal,
  settings,
  locked,
  generate,
  configure,
}: {
  lang: Lang;
  w: Workspace;
  settings: import("./types").GenerationSettings;
  locked: boolean;
  generate: (kind: string, prompt: string) => void;
  configure: () => void;
  view: (a: Artifact) => void;
  reveal: (path: string) => void;
}) {
  const tr = (k: Key) => t(lang, k);
  const exports = w.artifacts.filter((a) => a.kind === "export");
  return (
    <>
      <PageHeading title={tr("outputsTitle")} sub={tr("outputsSub")} />
      <AssetGenerator
        key={w.id}
        lang={lang}
        w={w}
        settings={settings}
        locked={locked}
        generate={generate}
        configure={configure}
      />
      <GeneratedAssets lang={lang} w={w} reveal={reveal} />
      {!w.artifacts.length ? (
        <Empty icon={FileText} title={tr("emptyOutputs")} />
      ) : null}
      {exports.length ? (
        <div className="export-list">
          {exports.toReversed().map((a) => (
            <article className="export-card" key={a.id}>
              <video
                src={mediaUrl(a.data.video!)}
                controls
                preload="metadata"
              />
              <div>
                <span className="output-type">{tr("rendered")}</span>
                <h3>{w.name}</h3>
                <p>
                  {seconds(a.data.duration || 0)} · {a.data.width} ×{" "}
                  {a.data.height} · {a.data.clipCount} {tr("clip")}
                </p>
                {a.data.hardwareEncoding ? (
                  <p>{tr("hardwareEncoding")}</p>
                ) : null}
                <time>{date(a.created, lang)}</time>
                <div>
                  <button
                    className="secondary small"
                    onClick={() => reveal(a.data.video!)}
                  >
                    <FolderOpen size={15} />
                    {tr("showFile")}
                  </button>
                  <button
                    className="text-button"
                    onClick={() => reveal(a.data.captions!)}
                  >
                    <FileText size={14} />
                    {tr("captions")}
                  </button>
                </div>
              </div>
            </article>
          ))}
        </div>
      ) : null}
      <div className="output-list">
        {w.artifacts
          .filter((a) => !["export", "voice", "image"].includes(a.kind))
          .toReversed()
          .map((a) => (
            <article key={a.id}>
              <div className={`output-icon ${a.kind}`}>
                {a.kind === "analysis" ? (
                  <BookOpen size={22} />
                ) : a.kind === "script" ? (
                  <FileText size={22} />
                ) : (
                  <Clapperboard size={22} />
                )}
              </div>
              <div className="output-info">
                <span className="output-type">
                  {tr(
                    a.kind === "analysis"
                      ? "analysis"
                      : a.kind === "script"
                        ? "script"
                        : "timeline",
                  )}
                </span>
                <h3>{a.title}</h3>
                <time>{date(a.created, lang)}</time>
              </div>
              <button className="text-button" onClick={() => view(a)}>
                {tr("view")}
                <ArrowRight size={15} />
              </button>
              <button
                className="icon-btn"
                title={tr("showFile")}
                onClick={() => reveal(a.path)}
              >
                <FolderOpen size={17} />
              </button>
            </article>
          ))}
      </div>
    </>
  );
}
export function RunsView({
  lang,
  w,
  logs,
  locked,
  start,
  view,
}: {
  lang: Lang;
  w: Workspace;
  logs: Record<string, string[]>;
  locked: boolean;
  start: (stage: string, id?: string) => void;
  view: (a: Artifact) => void;
}) {
  const tr = (k: Key) => t(lang, k);
  const labels: Record<string, Key> = {
    analyze: "study",
    adapt: "script",
    plan: "timeline",
    render: "rendered",
    voice: "voice",
    image: "image",
  };
  return (
    <>
      <PageHeading title={tr("historyTitle")} sub={tr("historySub")} />
      {!w.runs.length ? (
        <Empty icon={RefreshCw} title={tr("noRuns")} />
      ) : (
        <div className="run-list">
          {w.runs.toReversed().map((r) => (
            <article key={r.id}>
              <div className="run-header">
                <span className={`status ${r.status}`}>
                  {tr(r.status as Key)}
                </span>
                <h3>{tr(labels[r.stage] || "runs")}</h3>
                <time>{date(r.started, lang)}</time>
                {r.artifactId ? (
                  <button
                    className="text-button"
                    onClick={() => {
                      const a = w.artifacts.find((a) => a.id === r.artifactId);
                      if (a) view(a);
                    }}
                  >
                    {tr("view")}
                    <ArrowUpRight size={14} />
                  </button>
                ) : null}
                {r.status !== "running" &&
                ["analyze", "adapt", "plan"].includes(r.stage) ? (
                  <button
                    className="icon-btn"
                    disabled={locked}
                    title={tr("retry")}
                    onClick={() => start(r.stage)}
                  >
                    <RefreshCw size={16} />
                  </button>
                ) : null}
              </div>
              <p className="run-message">{r.message}</p>
              <details>
                <summary>
                  {tr("activity")} · {(logs[r.id] || r.logs).length}
                </summary>
                <pre>
                  {(logs[r.id] || r.logs).join("\n") || tr("emptyDetails")}
                </pre>
              </details>
            </article>
          ))}
        </div>
      )}
    </>
  );
}
