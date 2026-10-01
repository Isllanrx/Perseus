import { describe, expect, it } from "vitest";

import { buildId3, existingId3Size } from "./id3";
import { type Bytes, coverMime, looksLikeAudio, tagAudio, type TagInput } from "./index";
import { buildUdta, tagMp4 } from "./mp4";

const TAGS: TagInput = {
  title: "Faixa Um",
  artist: "Perseus",
  album: "Argonautas",
  trackNumber: 3,
  totalTracks: 12,
  year: 2024,
  comment: "https://soundcloud.com/perseus/um",
  genre: "Ambient",
  isrc: "BRXXX2400001",
  label: "Selo",
  composer: "Compositora",
  copyright: "2024 Selo",
  albumArtist: "Perseus",
  cover: { data: Uint8Array.of(0xff, 0xd8, 0xff, 0xe0), mime: "image/jpeg" },
};

const latin1 = (bytes: Uint8Array): string => Array.from(bytes, (byte) => String.fromCharCode(byte)).join("");

function box(type: string, ...payload: Uint8Array[]): Bytes {
  const size = 8 + payload.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(size);
  new DataView(out.buffer).setUint32(0, size);
  out.set(Uint8Array.from(type, (char) => char.charCodeAt(0)), 4);
  let offset = 8;
  for (const part of payload) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function concat(...parts: Uint8Array[]): Bytes {
  const out = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function fragment(tfhdFlags: number): Bytes {
  const tfhd = new Uint8Array(8);
  new DataView(tfhd.buffer).setUint32(0, tfhdFlags);
  return concat(box("moof", box("traf", box("tfhd", tfhd))), box("mdat", new Uint8Array(32)));
}

function mp3(frames = 10): Bytes {
  const frame = new Uint8Array(417);
  frame.set([0xff, 0xfb, 0x90, 0x00]);
  return concat(...Array.from({ length: frames }, () => frame));
}

describe("ID3v2.3", () => {
  it("writes a synchsafe header and every desktop frame", () => {
    const tag = buildId3(TAGS);
    expect(latin1(tag.subarray(0, 5))).toBe("ID3\u0003\u0000");
    expect(existingId3Size(tag)).toBe(tag.length);
    const text = latin1(tag);
    for (const id of ["TIT2", "TPE1", "TALB", "TRCK", "TYER", "COMM", "TCON", "TSRC", "TPUB", "TCOM", "TCOP", "TPE2", "APIC"]) {
      expect(text).toContain(id);
    }
    expect(text).toContain("3/12");
    expect(text).toContain("image/jpeg\u0000\u0003Cover\u0000");
  });

  it("uses UTF-16 with BOM only when Latin-1 is not enough", () => {
    const tag = buildId3({ ...TAGS, title: "Café", artist: "Пepceй", cover: null, comment: "Ω" });
    const text = latin1(tag);
    expect(text).toContain("TIT2\u0000\u0000\u0000\u0005\u0000\u0000\u0000Café");
    expect(text).toContain("\u0001ÿþ");
    expect(text).not.toContain("APIC");
  });

  it("measures an existing tag including the footer and ignores other data", () => {
    const header = Uint8Array.of(0x49, 0x44, 0x33, 4, 0, 0x10, 0, 0, 1, 0);
    expect(existingId3Size(header)).toBe(10 + 128 + 10);
    expect(existingId3Size(mp3(1))).toBe(0);
    expect(existingId3Size(Uint8Array.of(0x49, 0x44))).toBe(0);
  });

  it("replaces the stream tag instead of stacking a second one", () => {
    const old = buildId3({ ...TAGS, title: "Antigo", cover: null });
    const parts = tagAudio([concat(old, mp3(3)), mp3(2)], "mp3", TAGS);
    const file = concat(...parts);
    expect(latin1(file)).not.toContain("Antigo");
    expect(latin1(file.subarray(existingId3Size(file), existingId3Size(file) + 2))).toBe("ÿû");
    expect(file.length).toBe(buildId3(TAGS).length + 5 * 417);
  });
});

describe("MP4 ilst", () => {
  const init = (): Bytes =>
    concat(box("ftyp", Uint8Array.from("M4A ", (c) => c.charCodeAt(0))), box("moov", box("mvhd", new Uint8Array(12)), box("udta", new Uint8Array(4))));

  it("adds iTunes atoms to the init segment and keeps fragments untouched", () => {
    const segment = fragment(0x020000);
    const parts = tagMp4([init(), segment], TAGS);
    expect(parts).not.toBeNull();
    const [head, rest] = parts ?? [];
    const text = latin1(head ?? new Uint8Array());
    for (const atom of ["©nam", "©ART", "©alb", "trkn", "©day", "©cmt", "©gen", "©wrt", "cprt", "aART", "covr", "ISRC", "LABEL", "mdirappl"]) {
      expect(text).toContain(atom);
    }
    expect(text.match(/udta/g)).toHaveLength(1);
    const moovSize = new DataView((head ?? new Uint8Array(16)).buffer).getUint32(12);
    expect(12 + moovSize).toBe(head?.length);
    expect(rest).toBe(segment);
  });

  it("refuses fragments with absolute base offsets", () => {
    expect(tagMp4([init(), fragment(0x000001)], TAGS)).toBeNull();
    expect(tagAudio([init(), fragment(0x000001)], "mp4", TAGS)).toHaveLength(2);
  });

  it("shifts chunk offsets when moov comes before mdat", () => {
    const stco = new Uint8Array(12);
    new DataView(stco.buffer).setUint32(4, 1);
    new DataView(stco.buffer).setUint32(8, 100);
    const moov = box("moov", box("trak", box("mdia", box("minf", box("stbl", box("stco", stco))))));
    const file = concat(box("ftyp", new Uint8Array(4)), moov, box("mdat", new Uint8Array(16)));
    const [tagged] = tagMp4([file], TAGS) ?? [];
    const delta = (tagged?.length ?? 0) - file.length;
    const text = latin1(tagged ?? new Uint8Array());
    const at = text.indexOf("stco") + 4 + 8;
    expect(new DataView((tagged ?? new Uint8Array()).buffer).getUint32(at)).toBe(100 + delta);
  });

  it("gives up on files without moov and marks PNG covers", () => {
    expect(tagMp4([box("ftyp", new Uint8Array(4))], TAGS)).toBeNull();
    expect(tagMp4([], TAGS)).toBeNull();
    const png = buildUdta({ ...TAGS, cover: { data: Uint8Array.of(0x89, 0x50), mime: "image/png" } });
    expect(latin1(png)).toContain("covr");
    expect(coverMime(Uint8Array.of(0x89, 0x50, 0x4e, 0x47))).toBe("image/png");
    expect(coverMime(Uint8Array.of(0xff, 0xd8))).toBe("image/jpeg");
  });
});

describe("format checks", () => {
  it("recognizes each container and rejects junk", () => {
    expect(looksLikeAudio([mp3(5)], "mp3")).toBe(true);
    expect(looksLikeAudio([concat(buildId3(TAGS), mp3(5))], "mp3")).toBe(true);
    expect(looksLikeAudio([new Uint8Array(4096)], "mp3")).toBe(false);
    expect(looksLikeAudio([mp3(1).subarray(0, 100)], "mp3")).toBe(false);
    expect(looksLikeAudio([concat(box("ftyp", new Uint8Array(2000)))], "mp4")).toBe(true);
    expect(looksLikeAudio([new Uint8Array(2000)], "mp4")).toBe(false);
    expect(looksLikeAudio([concat(Uint8Array.from("OggS", (c) => c.charCodeAt(0)), new Uint8Array(2000))], "ogg")).toBe(true);
    expect(looksLikeAudio([], "ogg")).toBe(false);
  });

  it("leaves Ogg untouched and survives broken input", () => {
    const ogg = [concat(Uint8Array.from("OggS", (c) => c.charCodeAt(0)), new Uint8Array(10))];
    expect(tagAudio(ogg, "ogg", TAGS)).toEqual(ogg);
    expect(tagAudio([], "mp3", TAGS)).toEqual([]);
    const truncated = [Uint8Array.of(0, 0, 0, 1, 0x6d, 0x6f, 0x6f, 0x76)];
    expect(tagAudio(truncated, "mp4", TAGS)).toEqual(truncated);
  });
});
