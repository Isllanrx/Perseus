import type { Container } from "../types";
import { buildId3, existingId3Size } from "./id3";
import type { TagInput } from "./model";
import { type Bytes, tagMp4 } from "./mp4";

export type { Bytes } from "./mp4";
export type { Cover, TagInput } from "./model";
export { coverMime } from "./model";

const MIN_AUDIO_BYTES = 1024;
const SYNC_SEARCH_BYTES = 4096;

export function tagAudio(parts: readonly Bytes[], container: Container, tags: TagInput): Bytes[] {
  const [head, ...rest] = parts;
  if (!head) return [];
  try {
    if (container === "mp3") {
      const skip = Math.min(existingId3Size(head), head.length);
      return [buildId3(tags) as Bytes, head.subarray(skip), ...rest];
    }
    if (container === "mp4") return tagMp4(parts, tags) ?? [...parts];
  } catch {
  }
  return [...parts];
}

export function looksLikeAudio(parts: readonly Uint8Array[], container: Container): boolean {
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const head = parts[0];
  if (!head || total < MIN_AUDIO_BYTES) return false;
  const tag = (offset: number, text: string): boolean =>
    Array.from({ length: text.length }, (_, index) => text.charCodeAt(index)).every(
      (code, index) => head[offset + index] === code,
    );
  if (container === "mp4") return ["ftyp", "styp", "moov", "moof"].some((type) => tag(4, type));
  if (container === "ogg") return tag(0, "OggS");
  const start = Math.min(existingId3Size(head), head.length);
  const end = Math.min(head.length - 1, start + SYNC_SEARCH_BYTES);
  for (let index = start; index < end; index += 1) {
    if (head[index] === 0xff && ((head[index + 1] ?? 0) & 0xe0) === 0xe0) return true;
  }
  return false;
}
