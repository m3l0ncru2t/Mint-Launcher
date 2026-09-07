export type ModLoader = "vanilla" | "fabric" | "forge" | "quilt";

export interface ServerEntry {
  name: string;
  address: string;
}

export interface TextRun {
  text: string;
  color: string | null;
  bold: boolean;
  italic: boolean;
  underlined: boolean;
  strikethrough: boolean;
}

export interface PlayerSample {
  id: string;
  name: string;
}

export interface ServerStatus {
  motd: TextRun[];
  online: number | null;
  max: number | null;
  favicon: string | null;
  sample: PlayerSample[];
}

export interface OpEntry {
  uuid: string;
  name: string;
  level: number;
  bypassesPlayerLimit: boolean;
}

export interface WhitelistEntry {
  uuid: string;
  name: string;
}

export interface BannedPlayerEntry {
  uuid: string;
  name: string;
  created: string;
  source: string;
  expires: string;
  reason: string;
}

export type InstanceKind = "client" | "server";

export interface Instance {
  id: string;
  dirName: string;
  name: string;
  versionId: string;
  loader: ModLoader;
  loaderVersion: string | null;
  createdAt: string;
  lastPlayed: string | null;
  memoryMb: number;
  javaArgs: string | null;
  accountId: string | null;
  hasIcon: boolean;
  sortOrder: number;
  kind: InstanceKind;
  eulaAccepted: boolean;
  // Set for a server imported in place - its mods/world/config live here
  // directly instead of under Mint's own instances folder.
  externalDir: string | null;
}

export interface ServerDetection {
  versionId: string;
  loader: ModLoader;
  loaderVersion: string | null;
}

export interface VersionManifestEntry {
  id: string;
  type: "release" | "snapshot" | "old_beta" | "old_alpha";
  url: string;
  releaseTime: string;
}

export interface GameProfile {
  username: string;
  uuid: string;
  accessToken: string;
  userType: "legacy" | "msa";
}

export interface AccountSummary {
  id: string;
  username: string;
}

export interface SkinInfo {
  id: string;
  state: "ACTIVE" | "INACTIVE";
  url: string;
  variant: "CLASSIC" | "SLIM";
}

export interface CapeInfo {
  id: string;
  state: "ACTIVE" | "INACTIVE";
  url: string;
  alias: string;
}

export interface ProfileDetails {
  id: string;
  name: string;
  skins: SkinInfo[];
  capes: CapeInfo[];
}

export interface ThemeOpacity {
  sidebar: number;
  modsPanel: number;
}

export interface Settings {
  offlineUsername: string | null;
  microsoftClientId: string | null;
  backgroundTheme: string | null;
  themeOpacity: Record<string, ThemeOpacity>;
  customBackgroundNames: Record<string, string>;
  experimentalServerInstances: boolean;
  experimentalConfigsLogsTabs: boolean;
  remoteAdminEnabled: boolean;
  remoteAdminPort: number;
  remoteAdminInstanceId: string | null;
  spaciousInstanceView: boolean;
}

export interface CustomBackgroundInfo {
  id: string;
  name: string;
}

export interface PortableUpdateInfo {
  version: string;
  notes: string;
  downloadUrl: string;
}

export interface LaunchProgressEvent {
  instanceId: string;
  stage: "java" | "client" | "server" | "libraries" | "assets" | "launching" | "running" | "exited" | "error";
  message: string;
  current: number;
  total: number;
}

export interface InstanceLogEvent {
  instanceId: string;
  line: string;
}

export interface RunningInstance {
  pid: number;
  /** Absent for a running server instance - there's no account involved. */
  accountUuid: string | null;
  accountUsername: string | null;
}

export interface TpsInfo {
  tps: number;
  mspt: number;
}

export interface ProcessStats {
  cpuPercent: number;
  memoryMb: number;
  systemUsedMemoryMb: number;
  systemTotalMemoryMb: number;
}

export interface InstanceRunningEvent {
  instanceId: string;
  running: boolean;
  pid?: number;
  accountUuid?: string;
  accountUsername?: string;
}

export interface ModFile {
  fileName: string;
  size: number;
  enabled: boolean;
  isDependency: boolean;
}

/** A server another admin's machine is sharing over the remote admin API -
 * see remote_api.rs/remote_client.rs. Not a local `Instance` at all (no
 * folder, no local process) - `token` authorizes every call to `host:port`. */
export interface RemoteServerLink {
  id: string;
  name: string;
  host: string;
  port: number;
  token: string;
  versionId: string;
  loader: ModLoader;
  hasIcon: boolean;
}

export interface ResourcePackFile {
  fileName: string;
  size: number;
  enabled: boolean;
}

export interface ConfigEntry {
  fileName: string;
  size: number;
  isDir: boolean;
}

export interface LogEntry {
  fileName: string;
  size: number;
  modifiedAt: string | null;
}

export interface ResourcePackDetails {
  fileName: string;
  size: number;
  found: boolean;
  projectId: string | null;
  title: string | null;
  description: string | null;
  iconUrl: string | null;
  author: string | null;
  downloads: number | null;
  currentVersion: string | null;
  categories: string[];
  projectUrl: string | null;
}

export interface ModSearchResult {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  author: string;
  downloads: number;
  iconUrl: string | null;
  installed: boolean;
  upToDate: boolean | null;
}

export interface ModSearchPage {
  hits: ModSearchResult[];
  totalHits: number;
}

export interface ModProjectDetails {
  projectId: string;
  title: string;
  description: string;
  iconUrl: string | null;
  author: string | null;
  downloads: number;
  clientSide: string;
  serverSide: string;
  categories: string[];
  latestVersion: string | null;
  projectUrl: string;
}

export interface InstalledModInfo {
  projectId: string;
  title: string;
  fileName: string;
}

export interface InstallSummary {
  installed: InstalledModInfo[];
  alreadyInstalled: string[];
}

export interface ModUpdateInfo {
  fileName: string;
  projectId: string | null;
  title: string | null;
  iconUrl: string | null;
  currentVersion: string | null;
  latestVersion: string | null;
  updateAvailable: boolean;
  downloadUrl: string | null;
}

export interface ModDetails {
  fileName: string;
  size: number;
  enabled: boolean;
  isDependency: boolean;
  found: boolean;
  projectId: string | null;
  title: string | null;
  description: string | null;
  iconUrl: string | null;
  author: string | null;
  downloads: number | null;
  currentVersion: string | null;
  clientSide: string | null;
  serverSide: string | null;
  categories: string[];
  projectUrl: string | null;
}

export interface FabricLoaderInfo {
  version: string;
  stable: boolean;
}

export type LauncherKind = "official" | "multiMc" | "curseForge" | "modrinth";

export interface SuggestedPath {
  label: string;
  path: string;
}

export interface ImportProgressEvent {
  name: string;
  current: number;
  total: number;
  message: string;
}

export interface ImportCandidate {
  launcher: LauncherKind;
  name: string;
  sourcePath: string;
  minecraftDir: string;
  versionId: string;
  loader: ModLoader;
  loaderVersion: string | null;
  iconBase64: string | null;
  sizeBytes: number;
}

export interface LoginUrlInfo {
  url: string;
}
