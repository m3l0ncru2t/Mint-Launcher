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

export interface ServerStatus {
  motd: TextRun[];
  online: number | null;
  max: number | null;
  favicon: string | null;
}

export interface Instance {
  id: string;
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

export interface Settings {
  offlineUsername: string | null;
  microsoftClientId: string | null;
}

export interface LaunchProgressEvent {
  instanceId: string;
  stage: "client" | "libraries" | "assets" | "launching" | "exited" | "error";
  message: string;
  current: number;
  total: number;
}

export interface InstanceLogEvent {
  instanceId: string;
  line: string;
}

export interface ModFile {
  fileName: string;
  size: number;
  enabled: boolean;
  isDependency: boolean;
}

export interface ResourcePackFile {
  fileName: string;
  size: number;
  enabled: boolean;
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

export type LauncherKind = "official" | "multiMc" | "curseForge";

export interface SuggestedPath {
  label: string;
  path: string;
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
}

export interface LoginUrlInfo {
  url: string;
}
