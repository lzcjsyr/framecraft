import { invoke, isTauri, convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
export const native = isTauri();
export function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!native)
    return Promise.reject(
      new Error("Open the desktop app to use local folders and Claude Code."),
    );
  return invoke<T>(command, args);
}
export async function pick(
  kind: "folder" | "video" | "transcript" | "skill",
): Promise<string | null> {
  if (!native)
    throw new Error("File selection is available in the desktop app.");
  const result = await open({
    directory: kind === "folder",
    multiple: false,
    filters:
      kind === "video"
        ? [
            {
              name: "Video",
              extensions: ["mp4", "mov", "mkv", "avi", "webm", "m4v"],
            },
          ]
        : kind === "transcript"
          ? [{ name: "Transcript", extensions: ["srt", "vtt", "txt"] }]
          : kind === "skill"
            ? [{ name: "Skill", extensions: ["md"] }]
            : undefined,
  });
  return typeof result === "string" ? result : null;
}
export const mediaUrl = (path: string) => convertFileSrc(path);
export const seconds = (time: number) =>
  `${Math.floor(time / 60)
    .toString()
    .padStart(2, "0")}:${Math.floor(time % 60)
    .toString()
    .padStart(2, "0")}`;
export const date = (timestamp: number, lang: string) =>
  new Date(timestamp * 1000).toLocaleString(lang === "zh" ? "zh-CN" : "en-US", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
