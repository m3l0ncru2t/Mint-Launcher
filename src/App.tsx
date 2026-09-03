import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
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
import type { GameProfile, Instance, InstanceLogEvent, LaunchProgressEvent, Settings } from "./types";

export default function App() {
  const [loaded, setLoaded] = useState(false);
  const [instances, setInstances] = useState<Instance[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [profile, setProfile] = useState<GameProfile | null>(null);
  const [settings, setSettings] = useState<Settings>({ offlineUsername: null, microsoftClientId: null });
  const [showCreate, setShowCreate] = useState(false);
  const [showImportExternal, setShowImportExternal] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [instanceSettingsFor, setInstanceSettingsFor] = useState<Instance | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [progressByInstance, setProgressByInstance] = useState<Record<string, LaunchProgressEvent>>({});
  const [logsByInstance, setLogsByInstance] = useState<Record<string, string[]>>({});

  useEffect(() => {
    Promise.all([api.listInstances(), api.getActiveProfile(), api.getSettings()])
      .then(([inst, prof, settings]) => {
        setInstances(inst);
        setProfile(prof);
        setSettings(settings);
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
    return () => {
      unlistenProgress.then((f) => f());
      unlistenLog.then((f) => f());
    };
  }, []);

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

  async function handleImportInstance() {
    const path = await open({
      multiple: false,
      filters: [{ name: "Mint Launcher Backup", extensions: ["zip"] }],
    });
    if (!path) return;
    setImportError(null);
    try {
      const inst = await api.importInstance(path);
      refreshInstances(inst.id);
    } catch (e) {
      setImportError(String(e));
    }
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
        selectedId={selectedId}
        onSelect={setSelectedId}
        onNewInstance={() => setShowCreate(true)}
        onImportInstance={handleImportInstance}
        onImportFromLauncher={() => setShowImportExternal(true)}
        importError={importError}
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

      {showSettings && <SettingsDialog profile={profile} onClose={() => setShowSettings(false)} />}

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
