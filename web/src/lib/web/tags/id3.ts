import type { TagInput } from "./model";

const LATIN1_MAX = 0xff;

function isLatin1(text: string): boolean {
  for (let index = 0; index < text.length; index += 1) {
    if (text.charCodeAt(index) > LATIN1_MAX) return false;
  }
  return true;
}

function encodeText(text: string, terminated: boolean): { encoding: number; bytes: Uint8Array } {
  if (isLatin1(text)) {
    const bytes = new Uint8Array(text.length + (terminated ? 1 : 0));
    for (let index = 0; index < text.length; index += 1) bytes[index] = text.charCodeAt(index);
    return { encoding: 0, bytes };
  }
  const bytes = new Uint8Array(2 + text.length * 2 + (terminated ? 2 : 0));
  bytes[0] = 0xff;
  bytes[1] = 0xfe;
  for (let index = 0; index < text.length; index += 1) {
    const code = text.charCodeAt(index);
    bytes[2 + index * 2] = code & 0xff;
    bytes[3 + index * 2] = code >> 8;
  }
  return { encoding: 1, bytes };
}

function concat(parts: readonly Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function ascii(text: string): Uint8Array {
  return Uint8Array.from(text, (char) => char.charCodeAt(0));
}

function frame(id: string, body: Uint8Array): Uint8Array {
  const header = new Uint8Array(10);
  header.set(ascii(id), 0);
  new DataView(header.buffer).setUint32(4, body.length);
  return concat([header, body]);
}

function textFrame(id: string, value: string): Uint8Array {
  const { encoding, bytes } = encodeText(value, false);
  return frame(id, concat([Uint8Array.of(encoding), bytes]));
}

function commentFrame(value: string): Uint8Array {
  const latin = isLatin1(value);
  const description = latin ? Uint8Array.of(0) : Uint8Array.of(0xff, 0xfe, 0, 0);
  const text = encodeText(value, false).bytes;
  return frame("COMM", concat([Uint8Array.of(latin ? 0 : 1), ascii("eng"), description, text]));
}

function pictureFrame(data: Uint8Array, mime: string): Uint8Array {
  return frame("APIC", concat([Uint8Array.of(0), ascii(`${mime}\0`), Uint8Array.of(3), ascii("Cover\0"), data]));
}

function synchsafe(size: number): Uint8Array {
  return Uint8Array.of((size >> 21) & 0x7f, (size >> 14) & 0x7f, (size >> 7) & 0x7f, size & 0x7f);
}

export function existingId3Size(head: Uint8Array): number {
  if (head.length < 10 || head[0] !== 0x49 || head[1] !== 0x44 || head[2] !== 0x33) return 0;
  const size = [6, 7, 8, 9].reduce((total, index) => (total << 7) | ((head[index] ?? 0) & 0x7f), 0);
  const footer = ((head[5] ?? 0) & 0x10) !== 0 ? 10 : 0;
  return 10 + size + footer;
}

export function buildId3(tags: TagInput): Uint8Array {
  const frames: Uint8Array[] = [
    textFrame("TIT2", tags.title),
    textFrame("TPE1", tags.artist),
    textFrame("TALB", tags.album),
    textFrame("TRCK", `${String(tags.trackNumber)}/${String(tags.totalTracks)}`),
  ];
  if (tags.year !== null) frames.push(textFrame("TYER", String(tags.year)));
  if (tags.comment) frames.push(commentFrame(tags.comment));
  const extras: [string, string | null][] = [
    ["TCON", tags.genre],
    ["TSRC", tags.isrc],
    ["TPUB", tags.label],
    ["TCOM", tags.composer],
    ["TCOP", tags.copyright],
    ["TPE2", tags.albumArtist],
  ];
  for (const [id, value] of extras) {
    if (value) frames.push(textFrame(id, value));
  }
  if (tags.cover) frames.push(pictureFrame(tags.cover.data, tags.cover.mime));
  const body = concat(frames);
  return concat([ascii("ID3"), Uint8Array.of(3, 0, 0), synchsafe(body.length), body]);
}
