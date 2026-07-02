"use client";

// Terminal sheet. The picker lists BOTH kinds of shells:
//  • Terminal.app tabs — whatever is ALREADY open on the Mac (zero setup;
//    plain-text view polled from Terminal, typing via focus + injection);
//  • tmux panes — full-fidelity xterm.js attach (colors, TUIs, live resize).
// Tap one to open its viewer. xterm is dynamically imported so it never runs
// during the static build.

import { useEffect, useMemo, useRef, useState } from "react";
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

type Props = {
  onClose: () => void;
  status: Status;
  send: Send;
  sendBytes: (bytes: Uint8Array) => void;
  onOutput: (handler: OutputHandler) => () => void;
  onPanes: (handler: PanesHandler) => () => void;
  onTabs: (handler: TabsHandler) => () => void;
  onTabText: (handler: TabTextHandler) => () => void;
  onCreated: (handler: (p: { id: string; label: string }) => void) => () => void;
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
        style={{
          top,
          height: h ? `${h}px` : "100%",
          // Installed PWA draws under the iOS status bar; keep the sheet
          // header (and its buttons) tappable below it.
          paddingTop: "env(safe-area-inset-top)",
        }}
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
  onCreated,
  initialPane,
}: Props) {
  const [panes, setPanes] = useState<PaneInfo[] | null>(null);
  const [tabs, setTabs] = useState<TabInfo[] | null>(null);
  const [selectedTab, setSelectedTabState] = useState<TabInfo | null>(null);
  const [selected, setSelectedState] = useState<PaneInfo | null>(
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

  // Persist which shell is open, so a Safari page eviction (switching apps)
  // restores you straight back into it.
  const setSelected = (p: PaneInfo | null) => {
    setSelectedState(p);
    try {
      if (p) sessionStorage.setItem("pf.sel", JSON.stringify({ kind: "pane", pane: p }));
      else sessionStorage.removeItem("pf.sel");
    } catch {}
  };
  const setSelectedTab = (t: TabInfo | null) => {
    setSelectedTabState(t);
    try {
      if (t) sessionStorage.setItem("pf.sel", JSON.stringify({ kind: "tab", tab: t }));
      else sessionStorage.removeItem("pf.sel");
    } catch {}
  };

  useEffect(() => {
    if (initialPane) return; // explicit jump (Copilot card) wins over restore
    try {
      const raw = sessionStorage.getItem("pf.sel");
      if (!raw) return;
      const s = JSON.parse(raw);
      if (s.kind === "pane" && s.pane) setSelectedState(s.pane);
      else if (s.kind === "tab" && s.tab) setSelectedTabState(s.tab);
    } catch {}
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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

  // A shell created via "+ New" opens itself the moment the agent reports it.
  useEffect(
    () =>
      onCreated((p) =>
        setSelected({
          id: p.id,
          label: p.label,
          cmd: "",
          active: false,
          w: 0,
          h: 0,
        }),
      ),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [onCreated],
  );

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
        status={status}
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
      onNew={() => send(msg.tnew())}
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
  onNew,
  onPick,
  onPickTab,
  onClose,
}: {
  panes: PaneInfo[] | null;
  tabs: TabInfo[] | null;
  onRefresh: () => void;
  onNew: () => void;
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
            <button
              type="button"
              onClick={onNew}
              className="pf-press pf-accent select-none rounded-xl px-3 py-1 text-sm font-semibold"
            >
              + New
            </button>
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
                        {t.cwd ? (
                          <span className="text-emerald-300/80"> · {t.cwd}</span>
                        ) : null}
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
              None yet — tap <span className="text-emerald-300">+ New</span>{" "}
              above to open one right from your phone (crispest view: colors,
              live TUIs), then type <span className="font-mono">claude</span>.
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
                      <span className="truncate font-medium">
                        {p.label}
                        {p.cwd ? (
                          <span className="text-emerald-300/80"> · {p.cwd}</span>
                        ) : null}
                      </span>
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
  status,
  send,
  onTabText,
  onBack,
  onClose,
}: {
  tab: TabInfo;
  status: Status;
  send: Send;
  onTabText: (h: TabTextHandler) => () => void;
  onBack: () => void;
  onClose: () => void;
}) {
  const [hist, setHist] = useState("");
  const [screen, setScreen] = useState("");
  const [draft, setDraft] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  // Skip the reconnect effect's first firing only when we mounted already
  // connected (the mount effect selected the tab); after a real reconnect —
  // or a restore-on-reload that mounted before the socket opened — re-select.
  const skipNextConnect = useRef(status === "connected");

  // Send the composed draft into the tab. Inner newlines become Claude Code's
  // backslash+Enter (line break without submitting); the final Enter submits.
  const sendDraft = (text: string) => {
    const lines = text.split("\n");
    lines.forEach((line, i) => {
      if (line) send(msg.text(line));
      if (i < lines.length - 1) {
        send(msg.text("\\"));
        send(msg.key("enter"));
      }
    });
    send(msg.key("enter"));
  };

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

  // Re-select after the socket comes back (app switch, network blip).
  useEffect(() => {
    if (status !== "connected") return;
    if (skipNextConnect.current) {
      skipNextConnect.current = false;
      return;
    }
    send(msg.tabsel(tab.win, tab.tab));
  }, [status, tab, send]);

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
          className="pf-selectable min-h-0 flex-1 overflow-y-auto px-3 pt-2"
          style={{ touchAction: "pan-y" }}
        >
          <ColorText text={hist} dim />
          <ColorText text={screen || "Connecting to tab…"} />
        </div>

        {/* quick keys via the injection path (tab is focused on the Mac) */}
        <div className="pf-noscrollbar flex items-center gap-1.5 overflow-x-auto px-2 py-2">
          <CopyBtn
            getText={() => {
              const sel = window.getSelection()?.toString();
              return sel && sel.length > 0 ? sel : screen;
            }}
          />
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

        {/* photo lands in the compose box; ⏎ sends everything to the tab */}
        <div className="flex items-end gap-2 px-2 pb-[max(0.5rem,env(safe-area-inset-bottom))]">
          <PhotoButton
            onPath={(path) => setDraft((d) => (d ? `${d} ${path} ` : `${path} `))}
          />
          <div className="min-w-0 flex-1">
            <ComposeBar draft={draft} setDraft={setDraft} onSend={sendDraft} />
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
  const [draft, setDraft] = useState("");
  // See TabView: skip the first "connected" only if we mounted already
  // connected; a restore-on-reload mounts before the socket opens and needs
  // that first firing to actually attach.
  const skipNextConnect = useRef(status === "connected");

  // Send the composed draft as a bracketed paste (TUIs like Claude Code treat
  // pasted newlines as line breaks, not submissions), then Enter to submit.
  const sendDraft = (text: string) => {
    if (text) {
      const enc = new TextEncoder();
      const open = new Uint8Array([0x1b, 0x5b, 0x32, 0x30, 0x30, 0x7e]);
      const close = new Uint8Array([0x1b, 0x5b, 0x32, 0x30, 0x31, 0x7e]);
      const body = enc.encode(text);
      const all = new Uint8Array(open.length + body.length + close.length);
      all.set(open, 0);
      all.set(body, open.length);
      all.set(close, open.length + body.length);
      sendBytes(all);
    }
    sendBytes(new Uint8Array([0x0d]));
  };

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

  // Re-attach after a reconnect (or the first connect when restored from a
  // reload, where the mount-time attach was sent before the socket opened).
  useEffect(() => {
    if (status !== "connected") return;
    if (skipNextConnect.current) {
      skipNextConnect.current = false;
      return;
    }
    const t = termRef.current;
    send(msg.tsel(pane.id, t?.cols ?? 80, t?.rows ?? 24));
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
          <CopyBtn
            getText={() => {
              const t = termRef.current;
              if (!t) return "";
              const sel = t.getSelection?.();
              if (sel) return sel;
              // No selection → copy the visible screen.
              const buf = t.buffer.active;
              const lines: string[] = [];
              for (let y = 0; y < t.rows; y++) {
                const line = buf.getLine(buf.viewportY + y);
                if (line) lines.push(line.translateToString(true));
              }
              return lines.join("\n").trimEnd();
            }}
          />
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

        {/* photo lands in the compose box; ⏎ pastes + submits to the pane */}
        <div className="flex items-end gap-2 px-2 pb-[max(0.5rem,env(safe-area-inset-bottom))]">
          <PhotoButton
            onPath={(path) => setDraft((d) => (d ? `${d} ${path} ` : `${path} `))}
          />
          <div className="min-w-0 flex-1">
            <ComposeBar draft={draft} setDraft={setDraft} onSend={sendDraft} />
          </div>
        </div>
      </div>
    </SheetShell>
  );
}

/** Compose-then-send input: the phone keyboard's return key makes a NEW LINE
 *  in the draft (nothing is streamed live — what you see here is exactly what
 *  hasn't been sent yet); only the ⏎ button sends. Sending with an empty
 *  draft just presses Enter, handy for confirming prompts. The ↺ button
 *  cycles previously sent prompts into the box for editing/resending. */
function ComposeBar({
  draft,
  setDraft,
  onSend,
}: {
  draft: string;
  setDraft: (v: string) => void;
  onSend: (text: string) => void;
}) {
  const histIdx = useRef(0);
  const lastRecall = useRef<string | null>(null);
  const rows = Math.min(5, Math.max(1, draft.split("\n").length));

  const recall = async () => {
    const { loadHistory } = await import("../lib/history");
    const hist = loadHistory();
    if (hist.length === 0) return;
    // Fresh cycle unless the box still shows the last recalled prompt.
    histIdx.current =
      draft === lastRecall.current
        ? (histIdx.current + 1) % hist.length
        : 0;
    lastRecall.current = hist[histIdx.current];
    setDraft(hist[histIdx.current]);
  };

  const submit = async () => {
    if (draft.trim()) {
      const { pushHistory } = await import("../lib/history");
      pushHistory(draft);
    }
    onSend(draft);
    setDraft("");
    lastRecall.current = null;
  };

  return (
    <div className="flex items-end gap-2">
      <button
        type="button"
        aria-label="Previous prompt"
        onClick={recall}
        className="pf-press flex min-h-11 w-10 shrink-0 select-none items-center justify-center rounded-xl border border-white/10 bg-white/[0.06] text-base text-white/70"
      >
        ↺
      </button>
      <textarea
        rows={rows}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        placeholder="Compose here · ⏎ sends"
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

/** Terminal.app's scripting API strips ANSI colors, so re-colorize what
 *  matters most: diff lines. Claude Code edit diffs ("  42 +  code"), unified
 *  diffs ("+added"/"-removed"), and hunk headers get their colors back. */
function ColorText({ text, dim = false }: { text: string; dim?: boolean }) {
  const lines = useMemo(() => text.split("\n"), [text]);
  return (
    <pre
      className={`whitespace-pre-wrap break-words font-mono text-[11px] leading-snug ${
        dim ? "text-white/45" : "text-white/90"
      }`}
    >
      {lines.map((l, i) => {
        let cls = "";
        if (/^\s*\d+\s*\+/.test(l) || /^\+(?!\+\+)/.test(l))
          cls = "text-emerald-400";
        else if (/^\s*\d+\s*-/.test(l) || /^-(?!--)/.test(l))
          cls = "text-red-400";
        else if (/^\s*@@/.test(l)) cls = "text-cyan-300";
        const body = l + "\n";
        return cls ? (
          <span key={i} className={cls}>
            {body}
          </span>
        ) : (
          body
        );
      })}
    </pre>
  );
}

/** Copies the current selection (or the visible screen) to the phone
 *  clipboard, with a brief ✓ confirmation. */
function CopyBtn({ getText }: { getText: () => string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      type="button"
      aria-label="Copy"
      onClick={async () => {
        const { copyText } = await import("../lib/clipboard");
        if (await copyText(getText())) {
          setDone(true);
          setTimeout(() => setDone(false), 1200);
        }
      }}
      className={`pf-press shrink-0 select-none rounded-xl border px-3 py-1.5 font-mono text-sm ${
        done
          ? "border-emerald-400/40 bg-emerald-500/25 text-emerald-200"
          : "border-white/10 bg-white/[0.07] text-white/80"
      }`}
    >
      {done ? "✓" : "⧉"}
    </button>
  );
}
