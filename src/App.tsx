import { useEffect, useState } from "react";
import { api, RecordingStatus, Settings } from "./lib/api";
import Onboarding from "./screens/Onboarding";
import Home from "./screens/Home";
import Recording from "./screens/Recording";
import MeetingDetail from "./screens/MeetingDetail";
import SettingsScreen from "./screens/Settings";
import { Badge } from "./components/ui";

export type Route =
  | { name: "inicio" }
  | { name: "nova" }
  | { name: "reuniao"; id: string; seekMs?: number }
  | { name: "configuracoes" };

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [route, setRoute] = useState<Route>({ name: "inicio" });
  const [loadError, setLoadError] = useState<string | null>(null);
  const [recording, setRecording] = useState<RecordingStatus | null>(null);

  useEffect(() => {
    api
      .getSettings()
      .then(setSettings)
      .catch(() => setLoadError("Não foi possível carregar o aplicativo. Feche e abra o programa novamente."));
  }, []);

  // Enquanto houver gravação, a barra lateral mostra o estado e o usuário volta a ela com um clique.
  useEffect(() => {
    if (!settings?.onboardingCompleted) return;
    const tick = () => api.recordingStatus().then(setRecording).catch(() => setRecording(null));
    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  }, [settings?.onboardingCompleted]);

  if (loadError) {
    return (
      <main className="flex min-h-screen items-center justify-center p-8 text-center">
        <p role="alert" className="max-w-md text-[var(--muted)]">
          {loadError}
        </p>
      </main>
    );
  }
  if (!settings) {
    return (
      <main className="flex min-h-screen items-center justify-center">
        <p role="status" className="text-sm text-[var(--muted)]">
          Preparando o aplicativo…
        </p>
      </main>
    );
  }

  if (!settings.onboardingCompleted) {
    return (
      <Onboarding
        settings={settings}
        onDone={(s) => {
          setSettings(s);
          // Primeira reunião logo após a configuração, como pede a tela de conclusão.
          setRoute({ name: "nova" });
        }}
      />
    );
  }

  const nav: { name: Route["name"]; label: string }[] = [
    { name: "inicio", label: "Início" },
    { name: "nova", label: "Nova reunião" },
    { name: "configuracoes", label: "Configurações" },
  ];
  const current = route.name === "reuniao" ? "inicio" : route.name;

  return (
    <div className="flex min-h-screen">
      <aside className="flex w-56 shrink-0 flex-col gap-1 border-r border-black/5 bg-[var(--surface)] p-4 dark:border-white/5">
        <p className="mb-5 px-2 text-base font-semibold tracking-tight">IntegraMeet</p>
        {nav.map((n) => (
          <button
            key={n.name}
            onClick={() => setRoute({ name: n.name } as Route)}
            aria-current={current === n.name ? "page" : undefined}
            className={`rounded-lg px-3 py-2 text-left text-sm transition ${
              current === n.name ? "bg-[var(--accent)]/10 font-medium text-[var(--accent)]" : "text-[var(--muted)] hover:bg-black/5 hover:text-[var(--text)] dark:hover:bg-white/5"
            }`}
          >
            {n.label}
          </button>
        ))}

        {recording && (
          <button
            onClick={() => setRoute({ name: "nova" })}
            className="mt-auto flex flex-col gap-1 rounded-xl bg-red-500/10 p-3 text-left text-sm"
          >
            <span className="flex items-center gap-2 font-medium text-red-700 dark:text-red-300">
              <span className={`h-2 w-2 rounded-full ${recording.paused ? "bg-amber-500" : "animate-pulse bg-red-600"}`} />
              {recording.paused ? "Gravação pausada" : "Gravando agora"}
            </span>
            <span className="truncate text-xs text-[var(--muted)]">{recording.title}</span>
            {recording.problem && <Badge tone="warn">Atenção</Badge>}
          </button>
        )}
      </aside>

      <main className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-5xl px-8 py-8">
          {route.name === "inicio" && (
            <Home
              onNewMeeting={() => setRoute({ name: "nova" })}
              onOpenMeeting={(id, seekMs) => setRoute({ name: "reuniao", id, seekMs })}
            />
          )}
          {route.name === "nova" && (
            <Recording
              settings={settings}
              onSettings={setSettings}
              onStopped={(id) => setRoute({ name: "reuniao", id })}
              onBack={() => setRoute({ name: "inicio" })}
            />
          )}
          {route.name === "reuniao" && (
            <MeetingDetail
              key={route.id}
              id={route.id}
              initialSeekMs={route.seekMs}
              onBack={() => setRoute({ name: "inicio" })}
              onDeleted={() => setRoute({ name: "inicio" })}
            />
          )}
          {route.name === "configuracoes" && <SettingsScreen settings={settings} onSettings={setSettings} />}
        </div>
      </main>
    </div>
  );
}
