import { invoke } from "@tauri-apps/api/core";
import type {
  AccountSummary,
  FabricLoaderInfo,
  GameProfile,
  ImportCandidate,
  Instance,
  InstalledModInfo,
  InstallSummary,
  ModDetails,
  ModFile,
  ModLoader,
  ModProjectDetails,
  ModSearchPage,
  ModUpdateInfo,
  ProfileDetails,
  ResourcePackDetails,
  ResourcePackFile,
  ServerEntry,
  ServerStatus,
  Settings,
  SuggestedPath,
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

  listResourcepacks: (id: string) => invoke<ResourcePackFile[]>("list_resourcepacks", { id }),

  deleteResourcepack: (id: string, fileName: string) =>
    invoke<void>("delete_resourcepack", { id, fileName }),

  getResourcepacksDir: (id: string) => invoke<string>("get_resourcepacks_dir", { id }),

  searchResourcepacks: (id: string, query: string, offset: number) =>
    invoke<ModSearchPage>("search_resourcepacks", { id, query, offset }),

  installResourcepack: (id: string, projectId: string) =>
    invoke<InstalledModInfo>("install_resourcepack", { id, projectId }),

  toggleResourcepack: (id: string, fileName: string, enabled: boolean) =>
    invoke<void>("toggle_resourcepack", { id, fileName, enabled }),

  getResourcepackInfo: (id: string, fileName: string) =>
    invoke<ResourcePackDetails>("get_resourcepack_info", { id, fileName }),

  getResourcepackProjectInfo: (id: string, projectId: string) =>
    invoke<ModProjectDetails>("get_resourcepack_project_info", { id, projectId }),

  checkResourcepackUpdates: (id: string) => invoke<ModUpdateInfo[]>("check_resourcepack_updates", { id }),

  applyResourcepackUpdate: (id: string, oldFileName: string, downloadUrl: string) =>
    invoke<void>("apply_resourcepack_update", { id, oldFileName, downloadUrl }),

  getProjectInfo: (id: string, projectId: string) =>
    invoke<ModProjectDetails>("get_project_info", { id, projectId }),

  listServers: (id: string) => invoke<ServerEntry[]>("list_servers", { id }),

  saveServers: (id: string, servers: ServerEntry[]) =>
    invoke<ServerEntry[]>("save_servers", { id, servers }),

  pingServer: (address: string) => invoke<ServerStatus>("ping_server", { address }),

  updateInstanceSettings: (
    id: string,
    name: string,
    memoryMb: number,
    javaArgs: string | null,
    accountId: string | null,
  ) => invoke<Instance>("update_instance_settings", { id, name, memoryMb, javaArgs, accountId }),

  setInstanceIcon: (id: string, dataBase64: string) =>
    invoke<Instance>("set_instance_icon", { id, dataBase64 }),

  removeInstanceIcon: (id: string) => invoke<Instance>("remove_instance_icon", { id }),

  getInstanceIcon: (id: string) => invoke<string | null>("get_instance_icon", { id }),

  exportInstance: (id: string, destPath: string) => invoke<void>("export_instance", { id, destPath }),

  importInstance: (sourcePath: string) => invoke<Instance>("import_instance", { sourcePath }),

  suggestLauncherPaths: () => invoke<SuggestedPath[]>("suggest_launcher_paths"),

  scanExternalLauncher: (path: string) => invoke<ImportCandidate[]>("scan_external_launcher", { path }),

  importExternalInstance: (candidate: ImportCandidate) =>
    invoke<Instance>("import_external_instance", { candidate }),

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

  getProfileDetails: () => invoke<ProfileDetails>("get_profile_details"),

  uploadSkin: (variant: "classic" | "slim", dataBase64: string) =>
    invoke<ProfileDetails>("upload_skin", { variant, dataBase64 }),

  resetSkin: () => invoke<void>("reset_skin"),

  setCape: (capeId: string) => invoke<ProfileDetails>("set_cape", { capeId }),

  removeCape: () => invoke<void>("remove_cape"),

  getPlayerSkinUrl: (uuid: string) => invoke<string | null>("get_player_skin_url", { uuid }),

  launchInstance: (instanceId: string, serverAddress?: string) =>
    invoke<number>("launch_instance", { instanceId, serverAddress: serverAddress ?? null }),

  stopInstance: (instanceId: string) => invoke<void>("stop_instance", { instanceId }),
};
