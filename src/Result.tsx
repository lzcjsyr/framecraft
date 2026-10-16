import { Captions, FolderOpen, MonitorPlay, Play } from "lucide-react";
import type { CSSProperties } from "react";
import type { Artifact } from "./types";
import { mediaUrl, seconds } from "./api";
export function Result({
  artifacts,
  aspect,
  active,
  selected,
  onSelect,
  onReveal,
}: {
  artifacts: Artifact[];
  aspect: string;
  active: boolean;
  selected: string;
  onSelect: (id: string) => void;
  onReveal: (path: string) => void;
}) {
  const exports = artifacts.filter((a) => a.kind === "export" && a.data.video);
  const output = exports.find((a) => a.id === selected) || exports.at(-1);
  // Frame the stage at the video's own shape, or the requested one before any export.
  const [w, h] = output?.data.width && output.data.height
    ? [output.data.width, output.data.height]
    : aspect.split(":").map(Number);
  const ratio = w > 0 && h > 0 ? w / h : 16 / 9;
  return (
    <>
      <section className="result-panel" aria-labelledby="result-title">
        <div className="section-heading">
          <h2 id="result-title">
            <span className="section-icon" aria-hidden="true">
              <MonitorPlay size={14} />
            </span>
            Preview
          </h2>
          {exports.length > 1 ? (
            <select
              aria-label="Video version"
              value={output?.id}
              onChange={(e) => onSelect(e.target.value)}
            >
              {exports.map((a, i) => (
                <option key={a.id} value={a.id}>
                  Version {i + 1}
                </option>
              ))}
            </select>
          ) : null}
        </div>
        <div className="stage">
          <div
            className="video-frame"
            style={{ "--ratio": ratio } as CSSProperties}
          >
            {output?.data.video ? (
              <video
                key={output.id}
                controls
                preload="metadata"
                src={mediaUrl(output.data.video)}
              />
            ) : (
              <div className={active ? "video-empty working" : "video-empty"}>
                <span className="play-orb" aria-hidden="true">
                  <Play size={22} strokeWidth={1.6} />
                </span>
                <p>
                  {active
                    ? "Your agent is creating the video"
                    : "Your video will appear here"}
                </p>
              </div>
            )}
          </div>
        </div>
        {output ? (
          <div className="output-details">
            <strong>{output.title}</strong>
            <p>{output.data.summary}</p>
            <div className="output-actions">
              <span>
                {seconds(output.data.duration || 0)} · {output.data.width} ×{" "}
                {output.data.height}
              </span>
              <button
                className="small-button"
                onClick={() => onReveal(output.data.video!)}
              >
                <FolderOpen size={14} />
                Show MP4
              </button>
              {output.data.captions ? (
                <button
                  className="small-button"
                  onClick={() => onReveal(output.data.captions!)}
                >
                  <Captions size={14} />
                  Show SRT
                </button>
              ) : null}
            </div>
            {output.data.warnings?.length ? (
              <details className="warnings">
                <summary>
                  Production notes ({output.data.warnings.length})
                </summary>
                {output.data.warnings.map((note, i) => (
                  <p key={i}>{note}</p>
                ))}
              </details>
            ) : null}
          </div>
        ) : null}
      </section>
    </>
  );
}
