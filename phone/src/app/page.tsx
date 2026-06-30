"use client";

import { useState } from "react";
import { ControlButtons } from "../components/ControlButtons";
import { ScrollStrip } from "../components/ScrollStrip";
import { SettingsSheet } from "../components/SettingsSheet";
import { StatusBar } from "../components/StatusBar";
import { TerminalSheet } from "../components/TerminalSheet";
import { TextBar } from "../components/TextBar";
import { Trackpad } from "../components/Trackpad";
import { SettingsProvider } from "../lib/settings";
import { useAgent } from "../lib/useAgent";

export default function Page() {
  return (
    <SettingsProvider>
      <App />
    </SettingsProvider>
  );
}

function App() {
  const { status, send, onFrame } = useAgent();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [termOpen, setTermOpen] = useState(false);

  return (
    <main className="flex h-dvh flex-col gap-3 p-3">
      <StatusBar
        status={status}
        onSettings={() => setSettingsOpen(true)}
        onTerminal={() => setTermOpen(true)}
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
          onClose={() => setTermOpen(false)}
          send={send}
          onFrame={onFrame}
        />
      )}
    </main>
  );
}
