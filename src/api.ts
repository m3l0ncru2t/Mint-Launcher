import { invoke } from "@tauri-apps/api/core";
import type {
  AccountSummary,
  FabricLoaderInfo,
  GameProfile,
  Instance,
  InstallSummary,
  ModDetails,
  ModFile,
  ModLoader,
  ModProjectDetails,
  ModSearchPage,
  ModUpdateInfo,
  ServerEntry,
  Settings,
  VersionManifestEntry,
} from "./types";

export const api = {
  listInstances: () => invoke<Instance[]>("list_instances"),

  createInstance: (name: string, versionId: string, loader: ModLoader, loaderVersion: string | null) =>
    invoke<Instance>("create_instance", { name, versionId, loader, loaderVersion }),

  deleteInstance: (id: string) => invoke<void>("delete_instance", { id }),

  getInstance: (id: string) => invoke<Instance | null>("get_instance", { id }),

  listMods: (id: string) => invoke<ModFile[]>("list_mods", { id }),

  deleteMod: (id: string, fileName: string) => invoke<void>("delete_mod", { id, fileName }),

  toggleMod: (id: string, fileName: string, enabled: boolean) =>
    invoke<string>("toggle_mod", { id, fileName, enabled }),

  getModsDir: (id: string) => invoke<string>("get_mods_dir", { id }),

  checkModUpdates: (id: string) => invoke<ModUpdateInfo[]>("check_mod_updates", { id }),

  applyModUpdate: (id: string, oldFileName: string, downloadUrl: string) =>
    invoke<void>("apply_mod_update", { id, oldFileName, downloadUrl }),

  searchMods: (id: string, query: string, offset: number) =>
    invoke<ModSearchPage>("search_mods", { id, query, offset }),

  installMod: (id: string, projectId: string) =>
    invoke<InstallSummary>("install_mod", { id, projectId }),

  getModInfo: (id: string, fileName: string) =>
    invoke<ModDetails>("get_mod_info", { id, fileName }),

  getProjectInfo: (id: string, projectId: string) =>
    invoke<ModProjectDetails>("get_project_info", { id, projectId }),

  saveServers: (id: string, servers: ServerEntry[]) =>
    invoke<Instance>("save_servers", { id, servers }),

  getMinecraftVersions: () => invoke<VersionManifestEntry[]>("get_minecraft_versions"),

  getFabricLoaderVersions: (gameVersion: string) =>
    invoke<FabricLoaderInfo[]>("get_fabric_loader_versions", { gameVersion }),

  getSettings: () => invoke<Settings>("get_settings"),

  setMicrosoftClientId: (clientId: string | null) =>
    invoke<void>("set_microsoft_client_id", { clientId }),

  getActiveProfile: () => invoke<GameProfile | null>("get_active_profile"),

  signOut: () => invoke<void>("sign_out"),

  loginOffline: (username: string) => invoke<GameProfile>("login_offline", { username }),

  loginMicrosoft: () => invoke<GameProfile>("login_microsoft"),

  listAccounts: () => invoke<AccountSummary[]>("list_accounts"),

  switchAccount: (id: string) => invoke<GameProfile>("switch_account", { id }),

  removeAccount: (id: string) => invoke<void>("remove_account", { id }),

  launchInstance: (instanceId: string, serverAddress?: string) =>
    invoke<number>("launch_instance", { instanceId, serverAddress: serverAddress ?? null }),
};
