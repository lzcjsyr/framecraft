import { invoke, isTauri, convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Brief, Settings } from "./types";
export const native = isTauri();
export function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!native)
    return Promise.reject(
      new Error(
        "Open the Framecraft desktop app to use local files and the agent.",
      ),
    );
  return invoke<T>(command, args);
}
export async function pick(
  kind: "folder" | "video" | "skill",
): Promise<string[]> {
  if (!native)
    throw new Error("Open the desktop app to select a local folder or file.");
  const result = await open({
    directory: kind === "folder",
    multiple: false,
    filters:
      kind === "folder"
        ? undefined
        : [
            {
              name: kind,
              extensions:
                kind === "video"
                  ? ["mp4", "mov", "mkv", "avi", "webm", "m4v"]
                  : ["md"],
            },
          ],
  });
  return Array.isArray(result) ? result : result ? [result] : [];
}
export const mediaUrl = (path: string) =>
  native ? convertFileSrc(path) : path;
export const seconds = (time: number) =>
  `${Math.floor(time / 60)
    .toString()
    .padStart(2, "0")}:${Math.floor(time % 60)
    .toString()
    .padStart(2, "0")}`;
export const defaultBrief: Brief = {
  topic: "",
  direction: "",
  audience: "",
  platform: "",
  language: "简体中文",
  duration: 30,
  aspect: "16:9",
  narration: true,
};
export const defaultSettings: Settings = {
  mode: "mimo",
  baseUrl: "https://api.xiaomimimo.com/anthropic",
  model: "mimo-v2.5",
  claudePath: "",
  ffmpegPath: "",
  ffprobePath: "",
  whisperPath: "",
  generation: {
    voice: "mimo_default",
    voiceModel: "mimo-v2.5-tts",
    speechRate: 0,
    emotion: "neutral",
    imageProvider: "google_adc",
    imageModel: "gemini-3.1-flash-image",
    imageSize: "2560x1536",
    googleProject: "",
  },
};
// Native commands reject with strings; JS errors would otherwise show an "Error:" prefix.
export const describe = (e: unknown) =>
  e instanceof Error ? e.message : String(e);
