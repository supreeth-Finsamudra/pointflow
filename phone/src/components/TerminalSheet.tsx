"use client";

// Terminal sheet. The picker lists BOTH kinds of shells:
//  • Terminal.app tabs — whatever is ALREADY open on the Mac (zero setup;
//    plain-text view polled from Terminal, typing via focus + injection);
//  • tmux panes — full-fidelity xterm.js attach (colors, TUIs, live resize).
// Tap one to open its viewer. xterm is dynamically imported so it never runs
// during the static build.

import { useEffect, useRef, useState } from "react";
import "@xterm/xterm/css/xterm.css";
import { Send, msg } from "../lib/protocol";
import type {
  OutputHandler,
  PaneInfo,
  PanesHandler,
  Status,
  TabInfo,
  TabTextHandler,
  TabsHandler,
} from "../lib/useAgent";
import { useVisibleViewport } from "../lib/useViewport";
import { PhotoButton } from "./PhotoButton";
import { TextBar } from "./TextBar";

type Props = {
  onClose: () => void;
  status: Status;
  send: Send;
  sendBytes: (bytes: Uint8Array) => void;
  onOutput: (handler: OutputHandler) => () => void;
  onPanes: (handler: PanesHandler) => () => void;
  onTabs: (handler: TabsHandler) => () => void;
  onTabText: (handler: TabTextHandler) => () => void;
  /** Jump straight into this pane (from a Copilot card's "Open shell"). */
  initialPane?: { id: string; label: string } | null;
};

/**
 * Full-screen sheet chrome that plays nice with the on-screen keyboard: an
 * opaque backdrop always covers the whole screen (so the page underneath can
 * never bleed through), while the content column tracks the *visible* viewport
 * so inputs stay above the keyboard.
 */
function SheetShell({ children }: { children: React.ReactNode }) {
  const { h, top } = useVisibleViewport();
  return (
    <div className="fixed inset-0 z-50 bg-[#050508]">
      <div
        className="absolute left-0 flex w-full flex-col"
        style={{ top, height: h ? `${h}px` : "100%" }}
      >
        {children}
      </div>
    </div>
  );
}

function HeaderBtn({
  children,
  onPress,
}: {
  children: React.ReactNode;
  onPress: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onPress}
      className="pf-press select-none rounded-xl border border-white/10 bg-white/[0.07] px-3 py-1 text-sm text-white/80"
    >
      {children}
    </button>
  );
}

export function TerminalSheet({
  onClose,
  status,
  send,
  sendBytes,
  onOutput,
  onPanes,
  onTabs,
  onTabText,
  initialPane,
}: Props) {
  const [panes, setPanes] = useState<PaneInfo[] | null>(null);
  const [tabs, setTabs] = useState<TabInfo[] | null>(null);
  const [selectedTab, setSelectedTab] = useState<TabInfo | null>(null);
  const [selected, setSelected] = useState<PaneInfo | null>(
    initialPane
      ? {
          id: initialPane.id,
          label: initialPane.label,
          cmd: "",
          active: false,
          w: 0,
          h: 0,
        }
      : null,
  );

  useEffect(() => {
    const offPanes = onPanes(setPanes);
    const offTabs = onTabs(setTabs);
    send(msg.tlist());
    send(msg.tablist());
    return () => {
      offPanes();
      offTabs();
    };
  }, [onPanes, onTabs, send]);

  const refresh = () => {
    send(msg.tlist());
    send(msg.tablist());
  };

  // Refresh the lists whenever we (re)connect and aren't inside a viewer.
  useEffect(() => {
    if (status === "connected" && !selected && !selectedTab) {
      send(msg.tlist());
      send(msg.tablist());
    }
  }, [status, selected, selectedTab, send]);

  if (selected) {
    return (
      <PaneView
        pane={selected}
        status={status}
        send={send}
        sendBytes={sendBytes}
        onOutput={onOutput}
        onBack={() => setSelected(null)}
        onClose={onClose}
      />
    );
  }
  if (selectedTab) {
    return (
      <TabView
        tab={selectedTab}
        send={send}
        onTabText={onTabText}
        onBack={() => setSelectedTab(null)}
        onClose={onClose}
      />
    );
  }
  return (
    <PaneList
      panes={panes}
      tabs={tabs}
      onRefresh={refresh}
      onPick={setSelected}
      onPickTab={setSelectedTab}
      onClose={onClose}
    />
  );
}

function PaneList({
  panes,
  tabs,
  onRefresh,
  onPick,
  onPickTab,
  onClose,
}: {
  panes: PaneInfo[] | null;
  tabs: TabInfo[] | null;
  onRefresh: () => void;
  onPick: (p: PaneInfo) => void;
  onPickTab: (t: TabInfo) => void;
  onClose: () => void;
}) {
  return (
    <SheetShell>
      <div className="pf-rise flex min-h-0 flex-1 flex-col">
        <div className="pf-glass mx-3 mt-3 flex items-center justify-between rounded-2xl px-4 py-2.5">
          <span className="pf-brand font-mono text-base font-bold tracking-tight">
            {">_ shells"}
          </span>
          <div className="flex items-center gap-2">
            <HeaderBtn onPress={onRefresh}>↻</HeaderBtn>
            <HeaderBtn onPress={onClose}>Done</HeaderBtn>
          </div>
        </div>

        <div className="pf-noscrollbar min-h-0 flex-1 overflow-auto px-3 pb-6">
          {/* Already-open Terminal.app tabs — zero setup */}
          <p className="mb-2 mt-4 text-[11px] font-bold uppercase tracking-[0.18em] text-white/35">
            Terminal tabs
          </p>
          {tabs === null ? (
            <p className="text-sm text-white/40">Loading…</p>
          ) : tabs.length === 0 ? (
            <p className="text-sm text-white/40">No Terminal windows open.</p>
          ) : (
            <ul className="flex flex-col gap-2.5">
              {tabs.map((t) => (
                <li key={`${t.win}:${t.tab}`}>
                  <button
                    type="button"
                    onClick={() => onPickTab(t)}
                    className={`pf-press pf-glass flex w-full items-center justify-between rounded-2xl px-4 py-3.5 text-left ${
                      t.claude ? "border-violet-400/25" : ""
                    }`}
                  >
                    <span className="flex min-w-0 flex-col gap-0.5">
                      <span className="truncate font-medium">
                        {t.procs || "shell (idle)"}
                      </span>
                      <span className="font-mono text-xs text-white/40">
                        {t.tty} · window {t.win} tab {t.tab}
                      </span>
                    </span>
                    {t.claude ? (
                      <span className="shrink-0 rounded-full border border-violet-400/30 bg-violet-400/15 px-2.5 py-1 text-xs font-semibold text-violet-300">
                        ✳ claude
                      </span>
                    ) : t.busy ? (
                      <span className="shrink-0 rounded-full border border-emerald-400/25 bg-emerald-400/10 px-2.5 py-1 text-xs font-medium text-emerald-300">
                        ● running
                      </span>
                    ) : null}
                  </button>
                </li>
              ))}
            </ul>
          )}

          {/* tmux panes — full-fidelity mode */}
          <p className="mb-2 mt-7 text-[11px] font-bold uppercase tracking-[0.18em] text-white/35">
            tmux shells · best quality
          </p>
          {panes === null ? (
            <p className="text-sm text-white/40">Loading…</p>
          ) : panes.length === 0 ? (
            <div className="pf-glass rounded-2xl px-4 py-3 text-sm leading-relaxed text-white/50">
              None yet — for the crispest view (colors, live TUIs), run on the
              Mac:
              <pre className="mt-2 rounded-xl bg-black/40 px-3 py-2 font-mono text-xs text-emerald-300/80">
                tmux new -s work{"\n"}claude
              </pre>
            </div>
          ) : (
            <ul className="flex flex-col gap-2.5">
              {panes.map((p) => (
                <li key={p.id}>
                  <button
                    type="button"
                    onClick={() => onPick(p)}
                    className="pf-press pf-glass flex w-full items-center justify-between rounded-2xl px-4 py-3.5 text-left"
                  >
                    <span className="flex min-w-0 flex-col gap-0.5">
                      <span className="truncate font-medium">{p.label}</span>
                      <span className="font-mono text-xs text-white/40">
                        {p.cmd} · {p.w}×{p.h}
                      </span>
                    </span>
                    {p.status === "waiting" ? (
                      <span className="shrink-0 rounded-full border border-amber-400/30 bg-amber-400/15 px-2.5 py-1 text-xs font-semibold text-amber-300">
                        ⏸ needs you
                      </span>
                    ) : p.status === "done" ? (
                      <span className="shrink-0 rounded-full border border-emerald-400/25 bg-emerald-400/15 px-2.5 py-1 text-xs font-medium text-emerald-300">
                        ✓ done
                      </span>
                    ) : p.active ? (
                      <span className="pf-live h-2 w-2 shrink-0 rounded-full bg-emerald-400" />
                    ) : null}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </SheetShell>
  );
}

/** Plain-text live view of an already-open Terminal.app tab. The tab is
 *  focused on the Mac, so typing (TextBar/keys) uses the injection path. */
function TabView({
  tab,
  send,
  onTabText,
  onBack,
  onClose,
}: {
  tab: TabInfo;
  send: Send;
  onTabText: (h: TabTextHandler) => () => void;
  onBack: () => void;
  onClose: () => void;
}) {
  const [hist, setHist] = useState("");
  const [screen, setScreen] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);

  useEffect(() => {
    const off = onTabText((kind, text) => {
      if (kind === "hist") setHist(text);
      else setScreen(text);
    });
    send(msg.tabsel(tab.win, tab.tab));
    return () => {
      off();
      send(msg.tabstop());
    };
  }, [tab, send, onTabText]);

  // Keep pinned to the bottom unless the user scrolled up to read history.
  useEffect(() => {
    const el = scrollRef.current;
    if (el && pinned.current) el.scrollTop = el.scrollHeight;
  }, [hist, screen]);

  return (
    <SheetShell>
      <div className="pf-fade flex min-h-0 flex-1 flex-col">
        <div className="pf-glass mx-2 mt-2 flex items-center justify-between rounded-2xl px-3 py-2">
          <HeaderBtn onPress={onBack}>‹ Shells</HeaderBtn>
          <span className="truncate px-2 font-mono text-xs text-white/60">
            {tab.claude ? "✳ " : ""}
            {tab.procs || tab.tty}
          </span>
          <HeaderBtn onPress={onClose}>Done</HeaderBtn>
        </div>

        <div
          ref={scrollRef}
          onScroll={(e) => {
            const el = e.currentTarget;
            pinned.current =
              el.scrollHeight - el.scrollTop - el.clientHeight < 60;
          }}
          className="min-h-0 flex-1 overflow-y-auto px-3 pt-2"
          style={{ touchAction: "pan-y" }}
        >
          <pre className="whitespace-pre-wrap break-words font-mono text-[11px] leading-snug text-white/45">
            {hist}
          </pre>
          <pre className="whitespace-pre-wrap break-words font-mono text-[11px] leading-snug text-white/90">
            {screen || "Connecting to tab…"}
          </pre>
        </div>

        {/* quick keys via the injection path (tab is focused on the Mac) */}
        <div className="pf-noscrollbar flex items-center gap-1.5 overflow-x-auto px-2 py-2">
          <KeyBtn onPress={() => send(msg.key("escape"))}>Esc</KeyBtn>
          <KeyBtn onPress={() => send(msg.key("tab"))}>Tab</KeyBtn>
          <KeyBtn onPress={() => send(msg.key("enter"))}>⏎</KeyBtn>
          <KeyBtn onPress={() => send(msg.key("up"))}>↑</KeyBtn>
          <KeyBtn onPress={() => send(msg.key("down"))}>↓</KeyBtn>
          <KeyBtn onPress={() => send(msg.key("left"))}>←</KeyBtn>
          <KeyBtn onPress={() => send(msg.key("right"))}>→</KeyBtn>
          <KeyBtn onPress={() => send(msg.chord(["ctrl"], "c"))}>⌃C</KeyBtn>
          <KeyBtn onPress={() => send(msg.tabsel(tab.win, tab.tab))}>
            Focus
          </KeyBtn>
        </div>

        {/* photo + typing → injected into the focused tab */}
        <div className="flex items-end gap-2 px-2 pb-[max(0.5rem,env(safe-area-inset-bottom))]">
          <PhotoButton onPath={(path) => send(msg.text(`${path} `))} />
          <div className="min-w-0 flex-1">
            <TextBar send={send} />
          </div>
        </div>
      </div>
    </SheetShell>
  );
}

const QUICK_KEYS: { label: string; bytes: number[] }[] = [
  { label: "Esc", bytes: [0x1b] },
  { label: "Tab", bytes: [0x09] },
  { label: "⏎", bytes: [0x0d] },
  { label: "↑", bytes: [0x1b, 0x5b, 0x41] },
  { label: "↓", bytes: [0x1b, 0x5b, 0x42] },
  { label: "←", bytes: [0x1b, 0x5b, 0x44] },
  { label: "→", bytes: [0x1b, 0x5b, 0x43] },
  { label: "⌃C", bytes: [0x03] },
];

function PaneView({
  pane,
  status,
  send,
  sendBytes,
  onOutput,
  onBack,
  onClose,
}: {
  pane: PaneInfo;
  status: Status;
  send: Send;
  sendBytes: (b: Uint8Array) => void;
  onOutput: (h: OutputHandler) => () => void;
  onBack: () => void;
  onClose: () => void;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const termRef = useRef<any>(null);
  const fitRef = useRef<(() => void) | null>(null);
  const [font, setFont] = useState(12);
  const sawConnected = useRef(false);

  // Font-size changes refit the grid and re-sync tmux to the new cols/rows.
  useEffect(() => {
    const t = termRef.current;
    if (!t) return;
    t.options.fontSize = font;
    fitRef.current?.();
  }, [font]);

  useEffect(() => {
    let disposed = false;
    let off: (() => void) | null = null;
    let ro: ResizeObserver | null = null;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    let term: any = null;
    const enc = new TextEncoder();

    (async () => {
      const [{ Terminal }, { FitAddon }] = await Promise.all([
        import("@xterm/xterm"),
        import("@xterm/addon-fit"),
      ]);
      if (disposed || !hostRef.current) return;
      term = new Terminal({
        scrollback: 10000,
        fontFamily:
          'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
        fontSize: 12,
        theme: { background: "#050508", foreground: "#e5e5e5" },
        cursorBlink: false,
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.open(hostRef.current);
      termRef.current = term;

      // Size the terminal to the phone, then attach the pane at that size —
      // tmux repaints the full screen into exactly these dimensions.
      try {
        fit.fit();
      } catch {
        /* transient measurement error; defaults are fine */
      }

      term.onData((d: string) => sendBytes(enc.encode(d)));
      // Register the output sink *before* selecting so the history replay and
      // tmux's initial repaint aren't missed.
      off = onOutput((buf) => term.write(new Uint8Array(buf)));
      send(msg.tsel(pane.id, term.cols, term.rows));

      // Refit + tell tmux when the box changes (keyboard opens, rotation).
      fitRef.current = () => {
        try {
          fit.fit();
          send(msg.tresize(term.cols, term.rows));
          term.scrollToBottom();
        } catch {
          /* ignore */
        }
      };
      ro = new ResizeObserver(() => fitRef.current?.());
      ro.observe(hostRef.current);
    })();

    return () => {
      disposed = true;
      off?.();
      ro?.disconnect();
      term?.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, [pane, send, sendBytes, onOutput]);

  // Re-attach after a reconnect, but not on the initial connect (the effect
  // above already attached).
  useEffect(() => {
    if (status !== "connected") return;
    if (sawConnected.current) {
      const t = termRef.current;
      send(msg.tsel(pane.id, t?.cols ?? 80, t?.rows ?? 24));
    } else {
      sawConnected.current = true;
    }
  }, [status, pane.id, send]);

  return (
    <SheetShell>
      <div className="pf-fade flex min-h-0 flex-1 flex-col">
        <div className="pf-glass mx-2 mt-2 flex items-center justify-between rounded-2xl px-3 py-2">
          <HeaderBtn onPress={onBack}>‹ Shells</HeaderBtn>
          <span className="truncate px-2 font-mono text-xs text-white/60">
            {pane.label}
          </span>
          <HeaderBtn onPress={onClose}>Done</HeaderBtn>
        </div>

        {/* terminal — fitted to the phone; tmux reflows to this size */}
        <div className="relative min-h-0 flex-1 overflow-hidden px-1 pt-1">
          <div ref={hostRef} className="h-full w-full" />
        </div>

        {/* quick keys + font size */}
        <div className="pf-noscrollbar flex items-center gap-1.5 overflow-x-auto px-2 py-2">
          {QUICK_KEYS.map((k) => (
            <KeyBtn
              key={k.label}
              onPress={() => sendBytes(new Uint8Array(k.bytes))}
            >
              {k.label}
            </KeyBtn>
          ))}
          <div className="ml-auto flex shrink-0 items-center gap-1.5">
            <KeyBtn onPress={() => setFont((f) => Math.max(8, f - 1))}>−</KeyBtn>
            <span className="w-8 text-center text-xs tabular-nums text-white/50">
              {font}px
            </span>
            <KeyBtn onPress={() => setFont((f) => Math.min(20, f + 1))}>
              +
            </KeyBtn>
          </div>
        </div>

        {/* photo + type bar → raw bytes → the attached pane */}
        <div className="flex items-end gap-2 px-2 pb-[max(0.5rem,env(safe-area-inset-bottom))]">
          <PhotoButton
            onPath={(path) =>
              sendBytes(new TextEncoder().encode(`${path} `))
            }
          />
          <div className="min-w-0 flex-1">
            <TypeBar sendBytes={sendBytes} />
          </div>
        </div>
      </div>
    </SheetShell>
  );
}

/** Diff-based input so soft keyboards, autocorrect and dictation all turn into
 *  the right bytes streamed to the pane. */
function TypeBar({ sendBytes }: { sendBytes: (b: Uint8Array) => void }) {
  const last = useRef("");
  const [value, setValue] = useState("");
  const enc = useRef(new TextEncoder());

  const diff = (next: string) => {
    const prev = last.current;
    if (next === prev) return;
    let i = 0;
    const max = Math.min(next.length, prev.length);
    while (i < max && next[i] === prev[i]) i++;
    for (let b = 0; b < prev.length - i; b++) sendBytes(new Uint8Array([0x7f]));
    const typed = next.slice(i);
    if (typed) sendBytes(enc.current.encode(typed));
    last.current = next;
  };

  const submit = () => {
    sendBytes(new Uint8Array([0x0d]));
    last.current = "";
    setValue("");
  };

  return (
    <div className="flex items-end gap-2">
      <textarea
        rows={1}
        value={value}
        onChange={(e) => {
          const v = e.target.value;
          if (v.includes("\n")) {
            diff(v.slice(0, v.indexOf("\n")));
            submit();
            return;
          }
          diff(v);
          setValue(v);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            submit();
          }
        }}
        placeholder="Type here → this shell"
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
        className="min-h-11 flex-1 resize-none rounded-xl border border-white/10 bg-white/[0.06] px-3 py-2 text-base outline-none backdrop-blur placeholder:text-white/30 focus:border-emerald-300/40"
      />
      <button
        type="button"
        onClick={submit}
        className="pf-press pf-accent min-h-11 select-none rounded-xl px-4 font-semibold"
      >
        ⏎
      </button>
    </div>
  );
}

function KeyBtn({
  children,
  onPress,
}: {
  children: React.ReactNode;
  onPress: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onPress}
      className="pf-press shrink-0 select-none rounded-xl border border-white/10 bg-white/[0.07] px-3 py-1.5 font-mono text-sm text-white/80"
    >
      {children}
    </button>
  );
}
