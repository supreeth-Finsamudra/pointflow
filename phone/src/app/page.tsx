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
  const { status, send, sendBytes, onOutput, onPanes, onEvent, onTabs, onTabText } =
    useAgent();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [termOpen, setTermOpen] = useState(false);
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

  return (
    <main className="flex h-dvh flex-col gap-3 p-3">
      <StatusBar
        status={status}
        alert={event !== null}
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
