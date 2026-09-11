import { invoke } from "@tauri-apps/api/core";
import type {
  AccountSummary,
  BannedPlayerEntry,
  ConfigEntry,
  CustomBackgroundInfo,
  FabricLoaderInfo,
  GameProfile,
  ImportCandidate,
  Instance,
  InstalledModInfo,
  InstallSummary,
  LogEntry,
  ModDetails,
  ModFile,
  ModLoader,
  ModProjectDetails,
  ModSearchPage,
  ModUpdateInfo,
  OpEntry,
  PortableUpdateInfo,
  ProcessStats,
  ProfileDetails,
  RemoteServerLink,
  ResourcePackDetails,
  ResourcePackFile,
  RunningInstance,
  ServerDetection,
  ServerEntry,
  ServerStatus,
  Settings,
  SuggestedPath,
  TpsInfo,
  VersionManifestEntry,
  WhitelistEntry,
} from "./types";

export const api = {
  listInstances: () => invoke<Instance[]>("list_instances"),

  reorderInstances: (orderedIds: string[]) => invoke<Instance[]>("reorder_instances", { orderedIds }),

  createInstance: (name: string, versionId: string, loader: ModLoader, loaderVersion: string | null) =>
    invoke<Instance>("create_instance", { name, versionId, loader, loaderVersion }),

  createServerInstance: (
    name: string,
    versionId: string,
    loader: ModLoader,
    loaderVersion: string | null,
    eulaAccepted: boolean,
  ) => invoke<Instance>("create_server_instance", { name, versionId, loader, loaderVersion, eulaAccepted }),

  detectServerInstance: (sourcePath: string) => invoke<ServerDetection>("detect_server_instance", { sourcePath }),

  importServerFolder: (sourcePath: string, name: string, eulaAccepted: boolean) =>
    invoke<Instance>("import_server_folder", { sourcePath, name, eulaAccepted }),

  deleteInstance: (id: string) => invoke<void>("delete_instance", { id }),

  getInstance: (id: string) => invoke<Instance | null>("get_instance", { id }),

  listMods: (id: string) => invoke<ModFile[]>("list_mods", { id }),

  deleteMod: (id: string, fileName: string) => invoke<void>("delete_mod", { id, fileName }),

  toggleMod: (id: string, fileName: string, enabled: boolean) =>
    invoke<string>("toggle_mod", { id, fileName, enabled }),

  getModsDir: (id: string) => invoke<string>("get_mods_dir", { id }),

  openFolder: (path: string) => invoke<void>("open_folder", { path }),

  getServerProperties: (id: string) => invoke<Record<string, string>>("get_server_properties", { id }),

  saveServerProperties: (id: string, values: Record<string, string>) =>
    invoke<void>("save_server_properties", { id, values }),

  getOps: (id: string) => invoke<OpEntry[]>("get_ops", { id }),

  getWhitelist: (id: string) => invoke<WhitelistEntry[]>("get_whitelist", { id }),

  getBannedPlayers: (id: string) => invoke<BannedPlayerEntry[]>("get_banned_players", { id }),

  removeOpEntry: (id: string, name: string) => invoke<void>("remove_op_entry", { id, name }),

  removeWhitelistEntry: (id: string, name: string) => invoke<void>("remove_whitelist_entry", { id, name }),

  unbanPlayerEntry: (id: string, name: string) => invoke<void>("unban_player_entry", { id, name }),

  addOpEntry: (id: string, username: string) => invoke<void>("add_op_entry", { id, username }),

  addWhitelistEntry: (id: string, username: string) => invoke<void>("add_whitelist_entry", { id, username }),

  addBanEntry: (id: string, username: string, reason: string) =>
    invoke<void>("add_ban_entry", { id, username, reason }),

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

  listConfigFiles: (id: string) => invoke<ConfigEntry[]>("list_config_files", { id }),

  readConfigFile: (id: string, fileName: string) => invoke<string>("read_config_file", { id, fileName }),

  writeConfigFile: (id: string, fileName: string, content: string) =>
    invoke<void>("write_config_file", { id, fileName, content }),

  deleteConfigFile: (id: string, fileName: string) => invoke<void>("delete_config_file", { id, fileName }),

  getConfigDir: (id: string) => invoke<string>("get_config_dir", { id }),

  listLogFiles: (id: string) => invoke<LogEntry[]>("list_log_files", { id }),

  readLogFile: (id: string, fileName: string) => invoke<string>("read_log_file", { id, fileName }),

  getLogsDir: (id: string) => invoke<string>("get_logs_dir", { id }),

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

  listOnlinePlayers: (instanceId: string) => invoke<ServerStatus>("list_online_players", { instanceId }),

  getServerTps: (instanceId: string) => invoke<TpsInfo | null>("get_server_tps", { instanceId }),

  updateInstanceSettings: (
    id: string,
    name: string,
    memoryMb: number,
    javaArgs: string | null,
    accountId: string | null,
  ) => invoke<Instance>("update_instance_settings", { id, name, memoryMb, javaArgs, accountId }),

  upgradeInstanceLoader: (id: string, loader: ModLoader, loaderVersion: string) =>
    invoke<Instance>("upgrade_instance_loader", { id, loader, loaderVersion }),

  updateFabricLoaderVersion: (id: string, loaderVersion: string) =>
    invoke<Instance>("update_fabric_loader_version", { id, loaderVersion }),

  updateInstanceVersion: (id: string, versionId: string) =>
    invoke<Instance>("update_instance_version", { id, versionId }),

  setInstanceIcon: (id: string, dataBase64: string) =>
    invoke<Instance>("set_instance_icon", { id, dataBase64 }),

  removeInstanceIcon: (id: string) => invoke<Instance>("remove_instance_icon", { id }),

  getInstanceIcon: (id: string) => invoke<string | null>("get_instance_icon", { id }),

  getServerIcon: (id: string) => invoke<string | null>("get_server_icon", { id }),

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

  setExperimentalServerInstances: (enabled: boolean) =>
    invoke<void>("set_experimental_server_instances", { enabled }),

  setExperimentalConfigsLogsTabs: (enabled: boolean) =>
    invoke<void>("set_experimental_configs_logs_tabs", { enabled }),

  setRemoteAdminEnabled: (enabled: boolean) => invoke<void>("set_remote_admin_enabled", { enabled }),

  setRemoteAdminPort: (port: number) => invoke<void>("set_remote_admin_port", { port }),

  setRemoteAdminInstance: (instanceId: string | null) =>
    invoke<void>("set_remote_admin_instance", { instanceId }),

  setSpaciousInstanceView: (enabled: boolean) => invoke<void>("set_spacious_instance_view", { enabled }),

  remoteConnect: (host: string, port: number) => invoke<RemoteServerLink>("remote_connect", { host, port }),

  remoteReconnect: (id: string) => invoke<RemoteServerLink>("remote_reconnect", { id }),

  listRemoteServers: () => invoke<RemoteServerLink[]>("list_remote_servers"),

  removeRemoteServer: (id: string) => invoke<void>("remove_remote_server", { id }),

  setBackgroundTheme: (theme: string | null) => invoke<void>("set_background_theme", { theme }),

  setThemeOpacity: (themeId: string, sidebar: number, modsPanel: number) =>
    invoke<void>("set_theme_opacity", { themeId, sidebar, modsPanel }),

  addCustomBackground: (dataBase64: string, name: string) =>
    invoke<string>("add_custom_background", { dataBase64, name }),

  renameCustomBackground: (id: string, name: string) => invoke<void>("rename_custom_background", { id, name }),

  listCustomBackgrounds: () => invoke<CustomBackgroundInfo[]>("list_custom_backgrounds"),

  getCustomBackground: (id: string) => invoke<string | null>("get_custom_background", { id }),

  removeCustomBackground: (id: string) => invoke<void>("remove_custom_background", { id }),

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

  lookupPlayerUuid: (username: string) => invoke<string | null>("lookup_player_uuid", { username }),

  launchInstance: (instanceId: string, serverAddress?: string) =>
    invoke<number>("launch_instance", { instanceId, serverAddress: serverAddress ?? null }),

  stopInstance: (instanceId: string) => invoke<void>("stop_instance", { instanceId }),

  restartInstance: (instanceId: string) => invoke<void>("restart_instance", { instanceId }),

  killInstance: (instanceId: string) => invoke<void>("kill_instance", { instanceId }),

  sendInstanceCommand: (instanceId: string, command: string) =>
    invoke<void>("send_instance_command", { instanceId, command }),

  listRunningInstances: () => invoke<Record<string, RunningInstance>>("list_running_instances"),

  getProcessStats: (pid: number) => invoke<ProcessStats>("get_process_stats", { pid }),

  detectRunningServer: (instanceId: string) => invoke<boolean>("detect_running_server", { instanceId }),

  isPortable: () => invoke<boolean>("is_portable"),

  checkPortableUpdate: () => invoke<PortableUpdateInfo | null>("check_portable_update"),

  installPortableUpdate: (downloadUrl: string) =>
    invoke<void>("install_portable_update", { downloadUrl }),
};
