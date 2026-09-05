import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import { api } from "./api";
import { Sidebar } from "./components/Sidebar";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { CreateInstanceDialog } from "./components/CreateInstanceDialog";
import { InstanceDetail } from "./components/InstanceDetail";
import { ImportExternalDialog } from "./components/ImportExternalDialog";
import { InstanceSettingsDialog } from "./components/InstanceSettingsDialog";
import { LoginScreen } from "./components/LoginScreen";
import { SettingsDialog } from "./components/SettingsDialog";
import { UpdateBanner } from "./components/UpdateBanner";
import { BACKGROUND_THEMES } from "./themes";
import type {
  GameProfile,
  Instance,
  InstanceLogEvent,
  InstanceRunningEvent,
  LaunchProgressEvent,
  RunningInstance,
  Settings,
} from "./types";

export default function App() {
  const [loaded, setLoaded] = useState(false);
  const [instances, setInstances] = useState<Instance[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [profile, setProfile] = useState<GameProfile | null>(null);
  const [settings, setSettings] = useState<Settings>({
    offlineUsername: null,
    microsoftClientId: null,
    backgroundTheme: null,
    themeOpacity: {},
    customBackgroundNames: {},
  });
  const [showCreate, setShowCreate] = useState(false);
  const [showImportExternal, setShowImportExternal] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [instanceSettingsFor, setInstanceSettingsFor] = useState<Instance | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [progressByInstance, setProgressByInstance] = useState<Record<string, LaunchProgressEvent>>({});
  const [logsByInstance, setLogsByInstance] = useState<Record<string, string[]>>({});
  const [runningByInstance, setRunningByInstance] = useState<Record<string, RunningInstance>>({});

  useEffect(() => {
    Promise.all([api.listInstances(), api.getActiveProfile(), api.getSettings(), api.listRunningInstances()])
      .then(([inst, prof, settings, running]) => {
        setInstances(inst);
        setProfile(prof);
        setSettings(settings);
        setRunningByInstance(running);
        if (inst.length > 0) setSelectedId(inst[0].id);
      })
      .finally(() => setLoaded(true));
  }, []);

  useEffect(() => {
    const unlistenProgress = listen<LaunchProgressEvent>("launch-progress", (event) => {
      setProgressByInstance((prev) => ({ ...prev, [event.payload.instanceId]: event.payload }));
    });
    const unlistenLog = listen<InstanceLogEvent>("instance-log", (event) => {
      setLogsByInstance((prev) => {
        const existing = prev[event.payload.instanceId] ?? [];
        return { ...prev, [event.payload.instanceId]: [...existing, event.payload.line] };
      });
    });
    // Fired whenever a launch actually starts/stops a game process - kept
    // separate from `progressByInstance` (which also covers the download
    // stages before the game process exists) so the sidebar can show a
    // running badge for every instance, not just the selected one.
    const unlistenRunning = listen<InstanceRunningEvent>("instance-running-changed", (event) => {
      setRunningByInstance((prev) => {
        const next = { ...prev };
        const { instanceId, running, pid, accountUuid, accountUsername } = event.payload;
        if (running && pid !== undefined && accountUuid && accountUsername) {
          next[instanceId] = { pid, accountUuid, accountUsername };
        } else {
          delete next[instanceId];
        }
        return next;
      });
    });
    return () => {
      unlistenProgress.then((f) => f());
      unlistenLog.then((f) => f());
      unlistenRunning.then((f) => f());
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const theme = settings.backgroundTheme;

    async function apply() {
      const preset = BACKGROUND_THEMES.find((t) => t.id === theme);
      if (preset) {
        document.body.style.backgroundImage = preset.css;
        return;
      }
      if (theme) {
        // Not a known preset id - must be a previously-added custom
        // background, referenced by its own id.
        const url = await api.getCustomBackground(theme).catch(() => null);
        if (!cancelled) document.body.style.backgroundImage = url ? `url("${url}")` : "none";
        return;
      }
      document.body.style.backgroundImage = "none";
    }

    apply();
    return () => {
      cancelled = true;
    };
  }, [settings.backgroundTheme]);

  useEffect(() => {
    const key = settings.backgroundTheme ?? "default";
    const preset = BACKGROUND_THEMES.find((t) => t.id === key);
    const fallback = preset?.defaultOpacity ?? { sidebar: 0.82, modsPanel: 0.82 };
    const entry = settings.themeOpacity[key];
    document.documentElement.style.setProperty("--sidebar-alpha", String(entry?.sidebar ?? fallback.sidebar));
    document.documentElement.style.setProperty("--mods-panel-alpha", String(entry?.modsPanel ?? fallback.modsPanel));
  }, [settings.backgroundTheme, settings.themeOpacity]);

  function dismissProgress(instanceId: string) {
    setProgressByInstance((prev) => {
      const next = { ...prev };
      delete next[instanceId];
      return next;
    });
  }

  function refreshInstances(selectAfter?: string) {
    api.listInstances().then((inst) => {
      setInstances(inst);
      if (selectAfter) setSelectedId(selectAfter);
    });
  }

  function handleReorder(orderedIds: string[]) {
    setInstances((prev) => {
      const byId = new Map(prev.map((i) => [i.id, i]));
      return orderedIds.map((id) => byId.get(id)).filter((i): i is Instance => !!i);
    });
    api.reorderInstances(orderedIds).catch(() => refreshInstances());
  }

  async function performDelete(id: string) {
    await api.deleteInstance(id);
    const remaining = instances.filter((i) => i.id !== id);
    setInstances(remaining);
    if (selectedId === id) {
      setSelectedId(remaining[0]?.id ?? null);
    }
  }

  if (!loaded) {
    return (
      <div className="app-loading">
        <div className="spinner" />
        <div>Loading Mint Launcher…</div>
      </div>
    );
  }

  if (!profile) {
    return <LoginScreen settings={settings} onLoggedIn={setProfile} />;
  }

  const selectedInstance = instances.find((i) => i.id === selectedId) ?? null;

  return (
    <div className="app-shell">
      <UpdateBanner />
      <Sidebar
        instances={instances}
        runningByInstance={runningByInstance}
        selectedId={selectedId}
        onSelect={setSelectedId}
        onNewInstance={() => setShowCreate(true)}
        onImportInstance={() => setShowImportExternal(true)}
        onReorder={handleReorder}
        onOpenInstanceSettings={setInstanceSettingsFor}
        profile={profile}
        onProfileChange={setProfile}
        onSignOut={() => api.signOut().then(() => setProfile(null))}
        onOpenSettings={() => setShowSettings(true)}
      />

      {selectedInstance ? (
        <InstanceDetail
          key={selectedInstance.id}
          instance={selectedInstance}
          progress={progressByInstance[selectedInstance.id] ?? null}
          logLines={logsByInstance[selectedInstance.id] ?? []}
          onDelete={setConfirmDeleteId}
          onChanged={() => refreshInstances(selectedInstance.id)}
          onDismissProgress={() => dismissProgress(selectedInstance.id)}
          canPlay={!!profile}
        />
      ) : (
        <div className="main-panel">
          <div className="main-empty">Select or create an instance to get started.</div>
        </div>
      )}

      {showCreate && (
        <CreateInstanceDialog
          onClose={() => setShowCreate(false)}
          onCreated={(id) => {
            setShowCreate(false);
            refreshInstances(id);
          }}
        />
      )}

      {showSettings && (
        <SettingsDialog
          profile={profile}
          settings={settings}
          onSettingsChange={setSettings}
          onClose={() => setShowSettings(false)}
        />
      )}

      {showImportExternal && (
        <ImportExternalDialog
          onClose={() => setShowImportExternal(false)}
          onImported={(id) => {
            setShowImportExternal(false);
            refreshInstances(id);
          }}
        />
      )}

      {instanceSettingsFor && (
        <InstanceSettingsDialog
          instance={instanceSettingsFor}
          onClose={() => setInstanceSettingsFor(null)}
          onSaved={(updated) => {
            setInstanceSettingsFor(null);
            refreshInstances(updated.id);
          }}
          onIconChanged={(updated) => {
            setInstanceSettingsFor(updated);
            refreshInstances();
          }}
        />
      )}

      {confirmDeleteId && (
        <ConfirmDialog
          title="Delete instance?"
          message="This permanently deletes the instance, including all of its world saves, mods, and settings. There's no way to recover them afterward - back up any worlds you want to keep first."
          confirmLabel="Delete"
          danger
          onConfirm={() => {
            performDelete(confirmDeleteId);
            setConfirmDeleteId(null);
          }}
          onCancel={() => setConfirmDeleteId(null)}
        />
      )}
    </div>
  );
}
