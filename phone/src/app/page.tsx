"use client";

import { useEffect, useState } from "react";
import { ControlButtons } from "../components/ControlButtons";
import { CopilotCard } from "../components/CopilotCard";
import { ScrollStrip } from "../components/ScrollStrip";
import { SettingsSheet } from "../components/SettingsSheet";
import { StatusBar } from "../components/StatusBar";
import { TerminalSheet } from "../components/TerminalSheet";
import { TextBar } from "../components/TextBar";
import { Trackpad } from "../components/Trackpad";
import {
  enablePush,
  getPushState,
  registerSW,
  type PushState,
} from "../lib/push";
import { SettingsProvider } from "../lib/settings";
import { useAgent, type CopilotEvent } from "../lib/useAgent";

export default function Page() {
  return (
    <SettingsProvider>
      <App />
    </SettingsProvider>
  );
}

function App() {
  const {
    status,
    send,
    sendBytes,
    onOutput,
    onPanes,
    onEvent,
    onTabs,
    onTabText,
    onCreated,
  } = useAgent();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [termOpen, setTermOpenState] = useState(false);

  // Survive Safari evicting the page while you're in another app: remember
  // that the terminal sheet was open and restore it on reload.
  useEffect(() => {
    try {
      if (sessionStorage.getItem("pf.termOpen") === "1") setTermOpenState(true);
    } catch {
      /* private mode */
    }
  }, []);
  const setTermOpen = (v: boolean) => {
    setTermOpenState(v);
    try {
      if (v) sessionStorage.setItem("pf.termOpen", "1");
      else sessionStorage.removeItem("pf.termOpen");
    } catch {
      /* private mode */
    }
  };
  const [event, setEvent] = useState<CopilotEvent | null>(null);
  // Pane to auto-open (set by a card's "Open shell"); keying the sheet on it
  // remounts straight into that pane.
  const [jumpPane, setJumpPane] = useState<{ id: string; label: string } | null>(
    null,
  );

  useEffect(
    () =>
      onEvent((ev) => {
        setEvent(ev);
        // Android vibrates; iOS Safari ignores this (no vibration API).
        navigator.vibrate?.(200);
      }),
    [onEvent],
  );

  // Lock-screen push: register the SW, reflect state on the 🔔 button.
  const [push, setPush] = useState<PushState>("unsupported");
  useEffect(() => {
    registerSW().then(() => getPushState().then(setPush));
  }, []);
  const onPush = async () => {
    if (push === "needs-install") {
      alert(
        "To get lock-screen notifications on iPhone:\n\n1. Tap Share → Add to Home Screen\n2. Open PointFlow from the new icon\n3. Tap 🔕 again",
      );
      return;
    }
    if (push === "denied") {
      alert("Notifications are blocked for this site in Settings.");
      return;
    }
    if (push === "on") return;
    setPush(await enablePush());
  };

  return (
    <main
      className="flex h-dvh flex-col gap-3 p-3"
      style={{
        // Installed PWA draws under the iOS status bar; keep controls below it.
        paddingTop: "max(0.75rem, env(safe-area-inset-top))",
        paddingBottom: "max(0.75rem, env(safe-area-inset-bottom))",
      }}
    >
      <StatusBar
        status={status}
        alert={event !== null}
        push={push}
        onPush={onPush}
        onSettings={() => setSettingsOpen(true)}
        onTerminal={() => {
          setJumpPane(null);
          setTermOpen(true);
        }}
      />
      <div className="flex min-h-0 flex-1 gap-3">
        <Trackpad send={send} />
        <ScrollStrip send={send} />
      </div>
      <ControlButtons send={send} />
      <TextBar send={send} />
      <SettingsSheet open={settingsOpen} onClose={() => setSettingsOpen(false)} />
      {termOpen && (
        <TerminalSheet
          key={jumpPane?.id ?? "picker"}
          onClose={() => {
            setTermOpen(false);
            setJumpPane(null);
          }}
          status={status}
          send={send}
          sendBytes={sendBytes}
          onOutput={onOutput}
          onPanes={onPanes}
          onTabs={onTabs}
          onTabText={onTabText}
          onCreated={onCreated}
          initialPane={jumpPane}
        />
      )}
      {event && (
        <CopilotCard
          event={event}
          send={send}
          onOpen={(pane) => {
            setJumpPane(pane);
            setTermOpen(true);
          }}
          onDismiss={() => setEvent(null)}
        />
      )}
    </main>
  );
}
