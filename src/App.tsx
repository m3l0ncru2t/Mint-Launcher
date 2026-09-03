import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import { api } from "./api";
import { Sidebar } from "./components/Sidebar";
import { CreateInstanceDialog } from "./components/CreateInstanceDialog";
import { InstanceDetail } from "./components/InstanceDetail";
import { LoginScreen } from "./components/LoginScreen";
import { SettingsDialog } from "./components/SettingsDialog";
import type { GameProfile, Instance, InstanceLogEvent, LaunchProgressEvent, Settings } from "./types";

export default function App() {
  const [loaded, setLoaded] = useState(false);
  const [instances, setInstances] = useState<Instance[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [profile, setProfile] = useState<GameProfile | null>(null);
  const [settings, setSettings] = useState<Settings>({ offlineUsername: null, microsoftClientId: null });
  const [showCreate, setShowCreate] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
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

  function refreshInstances(selectAfter?: string) {
    api.listInstances().then((inst) => {
      setInstances(inst);
      if (selectAfter) setSelectedId(selectAfter);
    });
  }

  async function handleDelete(id: string) {
    if (!confirm("Delete this instance and all of its worlds/mods? This can't be undone.")) return;
    await api.deleteInstance(id);
    const remaining = instances.filter((i) => i.id !== id);
    setInstances(remaining);
    if (selectedId === id) {
      setSelectedId(remaining[0]?.id ?? null);
    }
  }

  if (!loaded) {
    return null;
  }

  if (!profile) {
    return (
      <LoginScreen settings={settings} onLoggedIn={setProfile} onOpenSettings={() => setShowSettings(true)} />
    );
  }

  const selectedInstance = instances.find((i) => i.id === selectedId) ?? null;

  return (
    <div className="app-shell">
      <Sidebar
        instances={instances}
        selectedId={selectedId}
        onSelect={setSelectedId}
        onNewInstance={() => setShowCreate(true)}
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
          onDelete={handleDelete}
          onChanged={() => refreshInstances(selectedInstance.id)}
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
          settings={settings}
          onClose={() => setShowSettings(false)}
          onSaved={(s) => {
            setSettings(s);
            setShowSettings(false);
          }}
        />
      )}
    </div>
  );
}
