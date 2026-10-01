import { downloadZip } from "client-zip";

export interface Artifact {
  name: string;
  blob: Blob;
}

export interface Sink {
  readonly kind: "directory" | "download";
  existing(folder: string, file: string): Promise<number | null>;
  write(folder: string, file: string, data: Blob): Promise<void>;
  finish(title: string): Promise<Artifact | null>;
}

const RESERVED = new Set(["\\", "/", ":", "*", "?", '"', "<", ">", "|"]);

export function safeFileName(name: string, fallback = "Perseus"): string {
  const replaced = Array.from(name, (char) => (RESERVED.has(char) || char.charCodeAt(0) < 0x20 ? "_" : char)).join("");
  const clean = replaced.replace(/^[\s.]+|[\s.]+$/g, "").slice(0, 120);
  return clean || fallback;
}

export class DirectorySink implements Sink {
  readonly kind = "directory";
  private readonly folders = new Map<string, Promise<FileSystemDirectoryHandle>>();

  constructor(private readonly root: FileSystemDirectoryHandle) {}

  private folder(name: string): Promise<FileSystemDirectoryHandle> {
    let handle = this.folders.get(name);
    if (!handle) {
      handle = this.root.getDirectoryHandle(name, { create: true });
      this.folders.set(name, handle);
      handle.catch(() => this.folders.delete(name));
    }
    return handle;
  }

  async existing(folder: string, file: string): Promise<number | null> {
    try {
      const handle = await (await this.folder(folder)).getFileHandle(file);
      const size = (await handle.getFile()).size;
      return size > 0 ? size : null;
    } catch {
      return null;
    }
  }

  async write(folder: string, file: string, data: Blob): Promise<void> {
    const handle = await (await this.folder(folder)).getFileHandle(file, { create: true });
    const writable = await handle.createWritable();
    try {
      await writable.write(data);
      await writable.close();
    } catch (error) {
      await writable.abort().catch(() => undefined);
      throw error;
    }
  }

  finish(): Promise<Artifact | null> {
    return Promise.resolve(null);
  }
}

interface Pending {
  folder: string;
  file: string;
  blob: Blob;
}

export class DownloadSink implements Sink {
  readonly kind = "download";
  private files: Pending[] = [];

  existing(): Promise<number | null> {
    return Promise.resolve(null);
  }

  write(folder: string, file: string, data: Blob): Promise<void> {
    this.files.push({ folder, file, blob: data });
    return Promise.resolve();
  }

  async finish(title: string): Promise<Artifact | null> {
    const files = this.files.sort((a, b) => a.folder.localeCompare(b.folder) || a.file.localeCompare(b.file));
    this.files = [];
    const [first] = files;
    if (!first) return null;
    if (files.length === 1) return { name: first.file, blob: first.blob };
    const folders = new Set(files.map((item) => item.folder));
    const single = folders.size === 1;
    const lastModified = new Date();
    const blob = await downloadZip(
      files.map((item) => ({
        name: single ? item.file : `${item.folder}/${item.file}`,
        input: item.blob,
        lastModified,
      })),
    ).blob();
    return { name: `${safeFileName(single ? first.folder : title)}.zip`, blob };
  }
}

export function saveArtifact(artifact: Artifact): void {
  const url = URL.createObjectURL(artifact.blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = artifact.name;
  anchor.rel = "noopener";
  anchor.style.display = "none";
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
