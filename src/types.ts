export type ModLoader = "vanilla" | "fabric" | "forge" | "quilt";

export interface ServerEntry {
  name: string;
  address: string;
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
  servers: ServerEntry[];
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

export interface LoginUrlInfo {
  url: string;
}
