import { ChevronDown, Film, Plus, Sparkles, X } from "lucide-react";
import type { Brief, Workspace } from "./types";
interface Props {
  brief: Brief;
  workspace: Workspace | null;
  disabled: boolean;
  onChange: (brief: Brief) => void;
  onImport: (kind: "reference" | "skill") => void;
  onRemove: (id: string) => void;
  onSkills: (ids: string[]) => void;
}
export function Guidance({
  brief,
  workspace: w,
  disabled,
  onChange,
  onImport,
  onRemove,
  onSkills,
}: Props) {
  const set = <K extends keyof Brief>(key: K, value: Brief[K]) =>
    onChange({ ...brief, [key]: value });
  return (
    <section className="guidance" aria-labelledby="guidance-title">
      <div className="section-heading">
        <h2 id="guidance-title">
          <span className="section-icon" aria-hidden="true">
            <Sparkles size={14} />
          </span>
          Guidance
        </h2>
      </div>
      <label className="prompt-field">
        <span>Goal</span>
        <textarea
          aria-label="What would you like to make?"
          value={brief.topic}
          disabled={disabled}
          maxLength={4000}
          placeholder="Describe the video, audience, and style."
          onChange={(e) => set("topic", e.target.value)}
        />
      </label>
      <div className="format-row">
        <select
          aria-label="Aspect ratio"
          value={brief.aspect}
          disabled={disabled}
          onChange={(e) => set("aspect", e.target.value)}
        >
          <option value="16:9">16:9</option>
          <option value="9:16">9:16</option>
          <option value="1:1">1:1</option>
        </select>
        <label className="duration-field">
          <input
            aria-label="Target duration in seconds"
            type="number"
            min={5}
            max={600}
            value={brief.duration}
            disabled={disabled}
            onChange={(e) => set("duration", Number(e.target.value))}
          />
          <span>s</span>
        </label>
        <select
          aria-label="Output language"
          value={brief.language}
          disabled={disabled}
          onChange={(e) => set("language", e.target.value)}
        >
          <option value="简体中文">中文</option>
          <option value="English">English</option>
        </select>
      </div>
      <div className="guidance-inputs">
        <div className="input-row">
          <span className="input-label">
            <Film size={14} aria-hidden="true" />
            Reference video
          </span>
          {w?.reference ? (
            <div className="attached-file">
              <span title={w.reference.path}>{w.reference.name}</span>
              <button
                className="icon-button"
                aria-label="Remove reference video"
                disabled={disabled}
                onClick={() => onRemove(w.reference!.id)}
              >
                <X size={15} />
              </button>
            </div>
          ) : (
            <button
              className="small-button"
              disabled={disabled}
              onClick={() => onImport("reference")}
            >
              Add video
            </button>
          )}
        </div>
        <div className="input-row">
          <details className="skill-picker menu">
            <summary>
              Skills
              {w?.selectedSkills.length ? (
                <span className="count">{w.selectedSkills.length}</span>
              ) : null}
              <ChevronDown size={14} aria-hidden="true" />
            </summary>
            <div className="skill-options">
              {w?.skills.map((sk) => (
                <label key={sk.id} title={sk.description}>
                  <input
                    type="checkbox"
                    disabled={disabled}
                    checked={w.selectedSkills.includes(sk.id)}
                    onChange={(e) =>
                      onSkills(
                        e.target.checked
                          ? [...w.selectedSkills, sk.id]
                          : w.selectedSkills.filter((id) => id !== sk.id),
                      )
                    }
                  />
                  {sk.name}
                </label>
              ))}
              {!w ? (
                <p>Skills will be available after choosing a workspace.</p>
              ) : !w.skills.length ? (
                <p>No skills yet. Use Add skill to import one.</p>
              ) : null}
            </div>
          </details>
          <button
            className="small-button"
            disabled={disabled}
            onClick={() => onImport("skill")}
          >
            <Plus size={14} />
            Add skill
          </button>
        </div>
      </div>
      {brief.direction || brief.audience || brief.platform ? (
        <details className="extra-guidance">
          <summary>More guidance</summary>
          {(["direction", "audience", "platform"] as const).map((key) => (
            <label key={key}>
              {key}
              <input
                disabled={disabled}
                value={brief[key]}
                onChange={(e) => set(key, e.target.value)}
              />
            </label>
          ))}
        </details>
      ) : null}
    </section>
  );
}
