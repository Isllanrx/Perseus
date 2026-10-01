import type { TagInput } from "./model";

export type Bytes = Uint8Array<ArrayBuffer>;

interface Box {
  type: string;
  start: number;
  size: number;
  header: number;
}

const CONTAINERS = new Set(["trak", "mdia", "minf", "stbl"]);
const BASE_DATA_OFFSET_PRESENT = 0x000001;

function view(data: Uint8Array): DataView {
  return new DataView(data.buffer, data.byteOffset, data.byteLength);
}

function boxes(data: Uint8Array, start: number, end: number): Box[] {
  const dv = view(data);
  const found: Box[] = [];
  let position = start;
  while (position + 8 <= end) {
    let size = dv.getUint32(position);
    let header = 8;
    if (size === 1) {
      if (position + 16 > end) break;
      size = Number(dv.getBigUint64(position + 8));
      header = 16;
    } else if (size === 0) {
      size = end - position;
    }
    if (size < header || position + size > end) break;
    const type = String.fromCharCode(...data.subarray(position + 4, position + 8));
    found.push({ type, start: position, size, header });
    position += size;
  }
  return found;
}

function latin1(text: string): Uint8Array {
  return Uint8Array.from(text, (char) => char.charCodeAt(0) & 0xff);
}

const utf8 = (text: string): Uint8Array => new TextEncoder().encode(text);

function box(type: string, ...payload: Uint8Array[]): Uint8Array {
  const size = 8 + payload.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(size);
  view(out).setUint32(0, size);
  out.set(latin1(type), 4);
  let offset = 8;
  for (const part of payload) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

const FULL_BOX = new Uint8Array(4);

function data(kind: number, payload: Uint8Array): Uint8Array {
  return box("data", Uint8Array.of(0, 0, 0, kind), new Uint8Array(4), payload);
}

function text(type: string, value: string): Uint8Array {
  return box(type, data(1, utf8(value)));
}

function freeform(name: string, value: string): Uint8Array {
  return box("----", box("mean", FULL_BOX, utf8("com.apple.iTunes")), box("name", FULL_BOX, utf8(name)), data(1, utf8(value)));
}

function trackNumber(number: number, total: number): Uint8Array {
  const payload = new Uint8Array(8);
  view(payload).setUint16(2, Math.min(number, 0xffff));
  view(payload).setUint16(4, Math.min(total, 0xffff));
  return box("trkn", data(0, payload));
}

export function buildUdta(tags: TagInput): Uint8Array {
  const items: Uint8Array[] = [
    text("©nam", tags.title),
    text("©ART", tags.artist),
    text("©alb", tags.album),
    trackNumber(tags.trackNumber, tags.totalTracks),
  ];
  if (tags.year !== null) items.push(text("©day", String(tags.year)));
  if (tags.comment) items.push(text("©cmt", tags.comment));
  const extras: [string, string | null][] = [
    ["©gen", tags.genre],
    ["©wrt", tags.composer],
    ["cprt", tags.copyright],
    ["aART", tags.albumArtist],
  ];
  for (const [type, value] of extras) {
    if (value) items.push(text(type, value));
  }
  if (tags.isrc) items.push(freeform("ISRC", tags.isrc));
  if (tags.label) items.push(freeform("LABEL", tags.label));
  if (tags.cover) items.push(box("covr", data(tags.cover.mime === "image/png" ? 14 : 13, tags.cover.data)));
  const handler = box("hdlr", FULL_BOX, new Uint8Array(4), latin1("mdirappl"), new Uint8Array(9));
  return box("udta", box("meta", FULL_BOX, handler, box("ilst", ...items)));
}

function hasAbsoluteFragments(part: Uint8Array): boolean {
  const dv = view(part);
  for (const moof of boxes(part, 0, part.length).filter((b) => b.type === "moof")) {
    for (const traf of boxes(part, moof.start + moof.header, moof.start + moof.size).filter((b) => b.type === "traf")) {
      for (const tfhd of boxes(part, traf.start + traf.header, traf.start + traf.size).filter((b) => b.type === "tfhd")) {
        if ((dv.getUint32(tfhd.start + tfhd.header) & 0xffffff & BASE_DATA_OFFSET_PRESENT) !== 0) return true;
      }
    }
  }
  return false;
}

function shiftChunkOffsets(moov: Uint8Array, start: number, end: number, delta: number): void {
  const dv = view(moov);
  for (const child of boxes(moov, start, end)) {
    const body = child.start + child.header;
    if (CONTAINERS.has(child.type)) {
      shiftChunkOffsets(moov, body, child.start + child.size, delta);
    } else if (child.type === "stco" || child.type === "co64") {
      const count = dv.getUint32(body + 4);
      const width = child.type === "stco" ? 4 : 8;
      for (let index = 0; index < count; index += 1) {
        const at = body + 8 + index * width;
        if (at + width > child.start + child.size) return;
        if (width === 4) dv.setUint32(at, dv.getUint32(at) + delta);
        else dv.setBigUint64(at, dv.getBigUint64(at) + BigInt(delta));
      }
    }
  }
}

export function tagMp4(parts: readonly Bytes[], tags: TagInput): Bytes[] | null {
  const head = parts[0];
  if (!head) return null;
  const top = boxes(head, 0, head.length);
  const moov = top.find((b) => b.type === "moov");
  if (moov?.header !== 8) return null;
  if (parts.some(hasAbsoluteFragments)) return null;

  const children = boxes(head, moov.start + 8, moov.start + moov.size).filter((child) => child.type !== "udta");
  const kept = children.map((child) => head.subarray(child.start, child.start + child.size));
  const rebuilt = box("moov", ...kept, buildUdta(tags));
  const delta = rebuilt.length - moov.size;
  const mdatAfter = top.some((b) => b.type === "mdat" && b.start > moov.start);
  if (delta !== 0 && mdatAfter) shiftChunkOffsets(rebuilt, 8, rebuilt.length, delta);

  const tagged = new Uint8Array(head.length + delta);
  tagged.set(head.subarray(0, moov.start), 0);
  tagged.set(rebuilt, moov.start);
  tagged.set(head.subarray(moov.start + moov.size), moov.start + rebuilt.length);
  return [tagged, ...parts.slice(1)];
}
