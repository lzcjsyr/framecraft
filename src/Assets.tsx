import { Folder, FolderOpen } from "lucide-react";
export function Assets({
  folder,
  disabled,
  narration,
  onNarration,
  onChoose,
}: {
  folder: string | null;
  disabled: boolean;
  narration: boolean;
  onNarration: (value: boolean) => void;
  onChoose: () => void;
}) {
  return (
    <section className="assets" aria-labelledby="assets-title">
      <div className="section-heading">
        <h2 id="assets-title">
          <span className="section-icon" aria-hidden="true">
            <Folder size={14} />
          </span>
          Assets
        </h2>
        <button
          className="outline-button"
          disabled={disabled}
          onClick={onChoose}
        >
          <FolderOpen size={16} />
          {folder ? "Change folder" : "Choose folder"}
        </button>
      </div>
      <div className={`asset-folder ${folder ? "selected" : ""}`}>
        {folder ? (
          <>
            <strong>
              {folder.split(/[\\/]/).filter(Boolean).at(-1) || folder}
            </strong>
            <code title={folder}>{folder}</code>
          </>
        ) : (
          <p>Put all related content in one folder.</p>
        )}
        <small>
          {folder
            ? "The agent reads this folder. Finished videos are saved here."
            : "Choose that folder to use it for your video."}
        </small>
      </div>
      <label className="narration-option">
        <input
          className="switch"
          type="checkbox"
          checked={narration}
          disabled={disabled}
          onChange={(e) => onNarration(e.target.checked)}
        />
        Add AI narration
      </label>
    </section>
  );
}
