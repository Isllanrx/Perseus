export interface Cover {
  data: Uint8Array;
  mime: "image/jpeg" | "image/png";
}

export interface TagInput {
  title: string;
  artist: string;
  album: string;
  trackNumber: number;
  totalTracks: number;
  year: number | null;
  comment: string | null;
  genre: string | null;
  isrc: string | null;
  label: string | null;
  composer: string | null;
  copyright: string | null;
  albumArtist: string | null;
  cover: Cover | null;
}

export function coverMime(data: Uint8Array): Cover["mime"] {
  return data[0] === 0x89 && data[1] === 0x50 && data[2] === 0x4e && data[3] === 0x47 ? "image/png" : "image/jpeg";
}
