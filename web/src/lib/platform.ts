export const IS_WEB = import.meta.env.MODE === "web";

export const DESKTOP_DOWNLOAD_URL = "https://github.com/Isllanrx/Perseus/releases/latest";

export function canPickFolder(): boolean {
  return !IS_WEB || (typeof window !== "undefined" && typeof window.showDirectoryPicker === "function");
}
