import { api } from "../api";

// The machine's public IP is the same for every server - looked up once per
// launcher run and shared by the header and the Server tab.
let cache: Promise<string> | null = null;

export function getPublicIpCached(): Promise<string> {
  cache ??= api.getPublicIp().catch((e) => {
    cache = null;
    throw e;
  });
  return cache;
}
