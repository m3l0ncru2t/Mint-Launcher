import { api } from "./api";
import type {
  InstalledModInfo,
  InstallSummary,
  ModFile,
  ModSearchPage,
  ModUpdateInfo,
  RemoteServerLink,
  ResourcePackFile,
  ServerStatus,
} from "./types";

/** Talks directly to another machine's Mint Launcher over the network (see
 * `remote_api.rs`) - plain `fetch`/`WebSocket` calls, no Tauri IPC involved,
 * since once `remote_connect` has proven identity and handed back a bearer
 * token, everything else is just an authenticated HTTP/WebSocket call.
 *
 * The host's session store is in-memory only (see `AppState.
 * remote_sessions`), so *any* restart of the host's own Mint - an update, a
 * crash, a manual restart - silently invalidates every admin's saved token
 * with no warning. Rather than surface that as an error, `request()` reruns
 * the join/hasJoined handshake itself the moment it sees a 401 and retries
 * once - `onTokenRefreshed` lets the caller persist the new token so this
 * doesn't have to happen again next launch. */
export function remoteApi(link: RemoteServerLink, onTokenRefreshed?: (token: string) => void) {
  const base = `http://${link.host}:${link.port}`;
  let currentToken = link.token;

  async function reconnect(): Promise<boolean> {
    try {
      const fresh = await api.remoteReconnect(link.id);
      currentToken = fresh.token;
      onTokenRefreshed?.(fresh.token);
      return true;
    } catch {
      return false;
    }
  }

  async function request(path: string, init?: RequestInit): Promise<Response> {
    const doFetch = () =>
      fetch(`${base}${path}`, { ...init, headers: { ...init?.headers, Authorization: `Bearer ${currentToken}` } });

    let resp = await doFetch();
    if (resp.status === 401 && (await reconnect())) {
      resp = await doFetch();
    }
    if (!resp.ok) {
      const message = await resp.text().catch(() => "");
      throw new Error(message || `${resp.status} ${resp.statusText}`);
    }
    return resp;
  }

  return {
    getInstance: () =>
      request("/instance").then(
        (r) =>
          r.json() as Promise<{ id: string; name: string; versionId: string; loader: string; hasIcon: boolean }>,
      ),

    getIcon: () => request("/icon").then((r) => r.json() as Promise<{ iconUrl: string | null }>),

    listMods: () => request("/mods").then((r) => r.json() as Promise<ModFile[]>),

    deleteMod: (fileName: string) => request(`/mods/${encodeURIComponent(fileName)}`, { method: "DELETE" }),

    toggleMod: (fileName: string, enabled: boolean) =>
      request(`/mods/${encodeURIComponent(fileName)}/toggle`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled }),
      }).then((r) => r.json() as Promise<{ fileName: string }>),

    searchMods: (query: string, offset: number) =>
      request(`/mods/search?query=${encodeURIComponent(query)}&offset=${offset}`).then(
        (r) => r.json() as Promise<ModSearchPage>,
      ),

    installMod: (projectId: string) =>
      request("/mods/install", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ projectId }),
      }).then((r) => r.json() as Promise<InstallSummary>),

    checkModUpdates: () => request("/mods/updates").then((r) => r.json() as Promise<ModUpdateInfo[]>),

    applyModUpdate: (oldFileName: string, downloadUrl: string) =>
      request("/mods/updates/apply", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ oldFileName, downloadUrl }),
      }),

    uploadMod: (file: File) => {
      const form = new FormData();
      form.append("file", file, file.name);
      return request("/mods", { method: "POST", body: form });
    },

    listResourcePacks: () => request("/resourcepacks").then((r) => r.json() as Promise<ResourcePackFile[]>),

    deleteResourcePack: (fileName: string) =>
      request(`/resourcepacks/${encodeURIComponent(fileName)}`, { method: "DELETE" }),

    toggleResourcePack: (fileName: string, enabled: boolean) =>
      request(`/resourcepacks/${encodeURIComponent(fileName)}/toggle`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled }),
      }),

    searchResourcePacks: (query: string, offset: number) =>
      request(`/resourcepacks/search?query=${encodeURIComponent(query)}&offset=${offset}`).then(
        (r) => r.json() as Promise<ModSearchPage>,
      ),

    installResourcePack: (projectId: string) =>
      request("/resourcepacks/install", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ projectId }),
      }).then((r) => r.json() as Promise<InstalledModInfo>),

    checkResourcePackUpdates: () =>
      request("/resourcepacks/updates").then((r) => r.json() as Promise<ModUpdateInfo[]>),

    applyResourcePackUpdate: (oldFileName: string, downloadUrl: string) =>
      request("/resourcepacks/updates/apply", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ oldFileName, downloadUrl }),
      }),

    uploadResourcePack: (file: File) => {
      const form = new FormData();
      form.append("file", file, file.name);
      return request("/resourcepacks", { method: "POST", body: form });
    },

    getConsoleTail: () => request("/console").then((r) => r.text()),

    /** Live console lines - `onLine` fires per line, `onError` when the
     * socket drops (e.g. the host went offline); returns a cleanup function.
     * Uses whatever the current token is at connect time - if it's stale,
     * the next successful `request()` call elsewhere (status/stats polling,
     * which run continuously alongside the console) will refresh it and a
     * later reconnect of this socket picks up the fresh one. */
    connectConsole: (onLine: (line: string) => void, onError?: () => void): (() => void) => {
      const ws = new WebSocket(`ws://${link.host}:${link.port}/console/ws?token=${encodeURIComponent(currentToken)}`);
      ws.onmessage = (event) => onLine(String(event.data));
      ws.onerror = () => onError?.();
      ws.onclose = () => onError?.();
      return () => ws.close();
    },

    getStatus: () => request("/status").then((r) => r.json() as Promise<{ running: boolean }>),

    getStats: () =>
      request("/stats").then(
        (r) =>
          r.json() as Promise<{
            cpuPercent: number;
            memoryMb: number;
            memoryLimitMb: number;
            systemUsedMemoryMb: number;
            systemTotalMemoryMb: number;
            tps: number | null;
            mspt: number | null;
          }>,
      ),

    getPlayers: () => request("/players").then((r) => r.json() as Promise<ServerStatus>),

    start: () => request("/start", { method: "POST" }),
    stop: () => request("/stop", { method: "POST" }),
    restart: () => request("/restart", { method: "POST" }),
    kill: () => request("/kill", { method: "POST" }),
  };
}
