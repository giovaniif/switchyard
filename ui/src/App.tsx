import { loadDefaults } from "./launchDefaults";
import { createContext, Fragment, useContext, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type DragEvent as ReactDragEvent, type KeyboardEvent, type PointerEvent as ReactPointerEvent, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import type { LoginProvider, LoginState, Question } from "./protocol";
import { answersFrom, noPick, toggle, typeOther, type Pick } from "./questions";
import type { AccountEntry, Agent, Attachment, PendingMessage, ChangedFile, Engine, EngineState, EngineStates, Host, HostEntry, Item, ModelInfo, Models, SessionMeta, Status, ToolItem, Usage } from "./protocol";
import { ROW_LABEL, type RowState, rowState, useSeen } from "./rowState";
import { SlackConversationPage, SlackSidebar } from "./SlackPage";
import { Subscriptions, UsageStrip, type UsageAccount } from "./UsageMeters";
import { AnchoredPopover } from "./AnchoredPopover";
import { shownHosts, useHiddenHosts } from "./sidebarHosts";
import { HOST_NAME_LIMIT, nameFromDestination } from "./addHost";
import { AgentAvatar, AgentBar, AgentForm, AgentPlaces, AgentRequests, AgentRunPickers, AgentsPage, AgentStart, EarlierConversations, NewAgentRedirect, agentSettings, useAgentMove, useAgentSave } from "./AgentsPage";
import { SlackIcons } from "./SlackIcon";
import { agentHost, agentOf, agentsAttention, agentStatus, conversationOf, handleOf, insertMention, isAgentConversation, isEarlierConversation, leadingMention, matchAgents, mentionAt, originLabel } from "./agents";
import { size, stateText } from "./hosts";
import { SettingsPage } from "./SettingsPage";
import { accountChoices, accountLabel, accountName, accountOf, choiceOnEngine, DEFAULT_ACCOUNT } from "./accounts";
import { accountsOn, branchKey, cleanGroupName, GROUP_NAME_MAX, sidebarLayout, homeOn, loginKey, hostName, hostOf, hostOnline, modelsOn, routeHash, usageOn, useYard, type Actions, type Route, type State, type UploadTarget } from "./store";
import { dragHasFiles, humanSize, isInlineImage, pastedFiles, tooLarge, tooMany } from "./attach";
import { TerminalPane } from "./TerminalPane";
import { SessionPullRequest } from "./SessionPullRequest";
import { SessionRename } from "./SessionRename";
import { SessionHandover } from "./SessionHandover";
import { PeekScope, PrLink } from "./PrHoverCard";
import { PR_URL_SOURCE, prLink, shortLabel } from "./prPeek";
import { ProcPane } from "./ProcPane";
import { draftOf, fenceSegments, keepDraft, lineCount, onAppend, quoteInComposer, quoteSegments, type Quote, type Snippet, splitComments, splitQuotes, withQuotes } from "./compose";
import { blockSegments, fencedBlocks, type MdTable } from "./markdown";
import { fileTarget } from "./fileLinks";
import { saveFile } from "./filePreview";
import { FileLinks, FileView, fileLabel } from "./FileView";
import { MermaidDiagram } from "./MermaidDiagram";
import { EngineIcon } from "./EngineIcon";
import { AutomationForm, AutomationsPage } from "./AutomationsPage";
import { ConnectorsPage } from "./ConnectorsPage";
import { WidgetFrame } from "./WidgetFrame";
import { hiddenFromSidebar, runsAttention } from "./automations";
import { dragHasSession, dropUpdate, SESSION_DRAG_TYPE, type DropTarget } from "./sidebarDrop";
import { addsProviders, canSteer, ENGINES, ENGINE_ACCOUNT, ENGINE_INSTALL, ENGINE_LABEL, ENGINE_LOGIN, ENGINE_NAME, pickerEngines, queuedHint, shownEffort } from "./engines";
import { didNotRun, duration, handoffLinks, layout, sessionSubagents, stillPinned, summarize, stepCommand, thoughtLine, toolLabel, type Row, type WorkItem } from "./thread";
import { voiceCue, type Cue } from "./voice";
import { useVoice } from "./useVoice";
import { MobileSetup, MobileSetupButton } from "./MobileSetup";
import { CopyButton } from "./CopyButton";
import { phoneTargets } from "./mobile";
import { isAgentHome, loadChoice, loadFolder, loadHost, readRecent, rememberSession, validChoice, type Choice } from "./sessionDefaults";
export { isAgentHome, loadChoice, readRecent } from "./sessionDefaults";
export type { Choice } from "./sessionDefaults";
import { NavigationPalette } from "./NavigationPalette";
import { navigationShortcut, type NavigationEntry } from "./navigation";
import { sendUpdateAction, useAppUpdate } from "./appUpdate";

/** Permission modes, as Claude Code names them, and what each means per engine. */
export const PERMISSIONS: ({ id: Permission; label: string } & Record<Engine, string>)[] = [
  {
    id: "ask",
    label: "Ask first",
    claude: "Asks before commands and edits",
    codex: "Sandboxed to the folder, asks to go further",
    opencode: "Asks before commands, edits and web fetches",
    grok: "Asks before commands and edits",
    gemini: "Asks before commands and edits",
  },
  {
    id: "acceptEdits",
    label: "Accept edits",
    claude: "Edits files freely, asks before commands",
    codex: "Edits in the folder freely, asks to go further",
    opencode: "Edits files freely, asks before commands",
    grok: "Edits files freely, asks before commands",
    gemini: "Edits files freely, asks before commands",
  },
  {
    id: "plan",
    label: "Plan only",
    claude: "Reads and plans, changes nothing",
    codex: "Read-only: looks, never changes",
    opencode: "OpenCode's plan agent: edits are refused",
    grok: "Grok's plan mode: edits are refused",
    gemini: "Edits are refused; commands ask for approval",
  },
  { id: "full", label: "Full access", claude: "Never asks. Runs anything", codex: "No sandbox, never asks", opencode: "Never asks. Runs anything", grok: "Never asks. Runs anything", gemini: "Never asks. Runs anything" },
];
export const STATUS_LABEL: Record<Status, string> = {
  idle: "Idle",
  queued: "Queued",
  running: "Running",
  needs_approval: "Needs approval",
  error: "Error",
};

export const modelName = (models: Models, engine: Engine, id: string) =>
  models[engine]?.find((m) => m.id === id)?.name ?? id;
const gb = (bytes: number) => (bytes / 1024 ** 3).toFixed(bytes > 100 * 1024 ** 3 ? 0 : 1);
const pct = (n: number) => `${Math.round(n)}%`;
export const tilde = (path: string, home: string) => (home && path.startsWith(home) ? "~" + path.slice(home.length) : path);
const capitalize = (s: string) => (s ? s[0].toUpperCase() + s.slice(1) : s);

/** What the phone's Session setup chip shows and says. */
export const setupSettings = (model: string, account: string, effort: string, permission: Permission | undefined) => ({
  model,
  effort,
  label: [model, account, capitalize(effort), (PERMISSIONS.find((p) => p.id === permission) ?? PERMISSIONS[0]).label].filter(Boolean).join(", "),
  full: permission === "full",
});

/* ---------- icons ---------- */

export const Icon = ({ d, size = 14, width = 2 }: { d: string; size?: number; width?: number }) => (
  <svg aria-hidden="true" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={width} strokeLinecap="round" strokeLinejoin="round">
    <path d={d} />
  </svg>
);
export const PLUS = "M12 5v14M5 12h14";
const CLOCK = "M12 7v5l3 2M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z";
export const IMPORT = "M12 3v12M7 10l5 5 5-5M5 21h14";
export const CHEVRON_RIGHT = "M9 6l6 6-6 6";
const CHEVRON_DOWN = "M6 9l6 6 6-6";
const ARROW_UP = "M12 19V5M5 12l7-7 7 7";
export const REFRESH = "M21 12a9 9 0 1 1-3-6.7M21 4v5h-5";
export const CLOUD = "M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z";
const PAPERCLIP = "M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48";
export const FILE = "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8ZM14 3v5h5";
const CLOSE = "M18 6 6 18M6 6l12 12";
const MORE = "M5 12h.01M12 12h.01M19 12h.01";
const REPLY = "M9 17 4 12l5-5M20 18v-2a4 4 0 0 0-4-4H4";
export const PIN = "M12 17v5M9 3h6l-1 6 4 4H6l4-4-1-6Z";
const ARCHIVE = "M3 4h18v4H3ZM5 8v11a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8M10 12h4";
const GEAR =
  "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2ZM15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z";
const FOLDER_PLUS = "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2ZM12 11v6M9 14h6";
const PENCIL = "M4 20h4L19 9l-4-4L4 16ZM13.5 6.5l4 4";
const TRASH = "M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3";
const MENU = "M4 6h16M4 12h16M4 18h16";
const LEFT_PANEL = "M3 5h18v14H3ZM9 5v14";
const RIGHT_PANEL = "M3 5h18v14H3ZM15 5v14";
const MIC = "M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3ZM19 10v2a7 7 0 0 1-14 0v-2M12 19v3";
const STOP = "M7 7h10v10H7Z";
const WAVES = "M2 10v3M6 6v11M10 3v18M14 8v7M18 5v13M22 10v3";
/** What the dot on the Automations row says. */
const RUNS_MARK = { approval: "A run needs approval", working: "A run is working" } as const;
const GROUP_MARK = { approval: "A session needs approval", working: "A session is working" } as const;
/** What the dot on the Agents row says. */
const AGENTS_MARK = { approval: "An agent needs approval", needsYou: "An agent needs you", working: "An agent is working", failed: "An agent's turn failed", answered: "An agent answered" } as const;

/** An agent's place: its one conversation, or the empty one before its first message. */
function AgentView({ state, actions, id }: { state: State; actions: Actions; id: string }) {
  const agent = state.agents.find((a) => a.id === id);
  const chat = agent ? conversationOf(agent, state.sessions) : undefined;
  // The agent asked for the user: seen once its conversation is in front of them.
  const asked = chat?.needsYou?.at;
  useEffect(() => {
    if (!asked) return;
    const look = () => {
      if (document.visibilityState === "visible" && document.hasFocus()) actions.agentSeen(id);
    };
    look();
    window.addEventListener("focus", look);
    document.addEventListener("visibilitychange", look);
    return () => {
      window.removeEventListener("focus", look);
      document.removeEventListener("visibilitychange", look);
    };
  }, [asked, id]);
  if (!agent) return <AgentsPage state={state} actions={actions} />;
  // What it said on a host it moved away from stays there, a click away.
  const earlier = <>
    <AgentPlaces state={state} actions={actions} agent={agent} />
    <EarlierConversations state={state} actions={actions} agent={agent} />
  </>;
  if (!chat) return <AgentStart key={agent.id} state={state} actions={actions} agent={agent} earlier={earlier} />;
  return (
    <PeekScope sessionId={chat.id} peek={actions.peekPr}>
      <SessionView key={chat.id} meta={chat} state={state} actions={actions} before={earlier} />
    </PeekScope>
  );
}

/** An at sign: agents, which a message names as @Name. */
const AT = "M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0ZM16 12v1.5a2.5 2.5 0 0 0 5 0V12a9 9 0 1 0-3.5 7.1";
/** A plug: the Connectors page. */
const PLUG = "M9 2v6M15 2v6M6 8h12v4a6 6 0 0 1-12 0ZM12 18v4";

/** Marks something on a remote host. Labelled when it is the only sign of it. */
export const Cloud = ({ size = 14, label }: { size?: number; label?: string }) =>
  label ? (
    <svg className="cloud" role="img" aria-label={label} width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <path d={CLOUD} />
    </svg>
  ) : (
    <span className="cloud">
      <Icon d={CLOUD} size={size} />
    </span>
  );

/* ---------- app shell ---------- */

const SIDEBAR_W = 248;
const PANE_W = 420;
/** Where the sidebar stops being a column and becomes a drawer (the same width as in styles.css). */
const NARROW = "(max-width: 700px)";
const WORKSPACE_NARROW = "(max-width: 1100px)";

/** Whether the sessions drawer is open, and how to open it, for each page's menu button. */
const DrawerContext = createContext<{ open: boolean; show(trigger: HTMLButtonElement): void }>({ open: false, show: () => {} });

export function App() {
  const { state, actions } = useYard();
  const { route } = state;
  const [draftUsageAccount, setDraftUsageAccount] = useState<UsageAccount>();
  const visibleHosts = shownHosts(["local", ...state.hosts.map((host) => host.id)], useHiddenHosts());
  const agentChat = route.page === "agent" ? state.agents.find((a) => a.id === route.id)?.chat : undefined;
  const selectedSession = route.page === "session" ? state.sessions[route.id] : agentChat ? state.sessions[agentChat] : undefined;
  const selectedUsageAccount = selectedSession ? { host: hostOf(selectedSession), engine: selectedSession.engine, account: accountOf(selectedSession) }
    : route.page === "new" ? draftUsageAccount : undefined;
  const usage = {
    sources: visibleHosts.map((host) => ({ host, name: hostName(state, host), usage: usageOn(state, host), accounts: accountsOn(state, host), engines: state.engines[host], online: hostOnline(state, host) })),
    selected: selectedUsageAccount,
    onManageAccounts: () => actions.go({ page: "connectors" }),
    onManage: (host: string) => actions.go({ page: "hosts", host, tab: "engines" }),
    onRefresh: actions.refreshUsage,
  };
  const [importing, setImporting] = useState(false);
  const [navigating, setNavigating] = useState(false);
  const navigationOrigin = useRef<HTMLElement | null>(null);
  useLayoutEffect(() => {
    // The app is no longer inert after the palette unmounts. A destination
    // dialog may already have focused its own input; leave that focus alone.
    if (!navigating && navigationOrigin.current?.isConnected && document.activeElement === document.body) navigationOrigin.current.focus({ preventScroll: true });
  }, [navigating]);
  // Older shells and browsers still need the sidebar's entry points.
  const nativeMenu = (window as any).__SWITCHYARD_APP_MENU__ === true;
  const appUpdate = useAppUpdate();
  const [sidebarW, setSidebarW] = useWidth("yard.sidebarWidth", SIDEBAR_W);
  const [sidebarCollapsed, setSidebarCollapsed] = usePanelCollapsed("yard.sidebarCollapsed");
  const narrow = useMedia(NARROW);
  const sidebarHidden = !narrow && sidebarCollapsed;
  const sidebarCollapseLabel = narrow ? "Close sessions" : "Collapse sessions sidebar";
  const sidebarExpandButton = useRef<HTMLButtonElement>(null);
  const sidebarCollapseButton = useRef<HTMLButtonElement>(null);
  const drawerTrigger = useRef<HTMLButtonElement>(null);
  // Below NARROW the sidebar is a drawer over the page. Picking anything in
  // it, the backdrop or Escape closes it; so does crossing the breakpoint, so
  // it never comes back open by surprise.
  const [drawer, setDrawer] = useState(false);
  const closeDrawer = () => {
    setDrawer(false);
    requestAnimationFrame(() => drawerTrigger.current?.focus({ preventScroll: true }));
  };
  const toggleSidebar = () => {
    const nextButton = sidebarCollapsed ? sidebarCollapseButton : sidebarExpandButton;
    setSidebarCollapsed(!sidebarCollapsed);
    requestAnimationFrame(() => nextButton.current?.focus({ preventScroll: true }));
  };
  const openNavigation = () => {
    if (!navigating) navigationOrigin.current = document.activeElement as HTMLElement | null;
    setDrawer(false);
    setImporting(false);
    setNavigating(true);
  };
  const openImport = () => {
    setDrawer(false);
    setNavigating(false);
    setImporting(true);
  };
  const pickNavigation = (entry: NavigationEntry) => {
    setNavigating(false);
    if (entry.action === "import") openImport();
    else if (entry.route) actions.go(entry.route);
  };
  useEffect(() => {
    const key = (e: globalThis.KeyboardEvent) => {
      if (!navigationShortcut(e, /Mac|iPhone|iPad/.test(navigator.platform))) return;
      e.preventDefault();
      e.stopPropagation();
      if (navigating) setNavigating(false);
      else openNavigation();
    };
    const menu = (e: Event) => {
      setDrawer(false);
      switch ((e as CustomEvent).detail) {
        case "navigation": openNavigation(); break;
        case "import": openImport(); break;
        case "settings":
          setNavigating(false);
          setImporting(false);
          actions.go({ page: "settings" });
          break;
      }
    };
    window.addEventListener("keydown", key, true);
    window.addEventListener("switchyard:navigate", menu);
    return () => {
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("switchyard:navigate", menu);
    };
  }, [actions, navigating]);
  useEscape(drawer, closeDrawer);
  useEffect(() => {
    const mq = window.matchMedia(NARROW);
    const onChange = () => setDrawer(false);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  // Routes that change without a pick (an import's or a notification's session opening) close it too.
  const where = routeHash(route);
  useEffect(() => setDrawer(false), [where]);
  // A refusal of something picked in it (a row's Delete…) lands in the banner
  // over the page, which the open drawer would hide behind its scrim.
  const errorsSeen = useRef(state.errorCount);
  useEffect(() => {
    if (state.errorCount === errorsSeen.current) return;
    errorsSeen.current = state.errorCount;
    if (drawer) closeDrawer();
  }, [state.errorCount]);
  const drawerCtx = useMemo(() => ({
    open: drawer,
    show: (trigger: HTMLButtonElement) => {
      drawerTrigger.current = trigger;
      setDrawer(true);
    },
  }), [drawer]);
  const sidebarActions = useMemo<Actions>(
    () => ({
      ...actions,
      go: (r: Route) => {
        setDrawer(false);
        actions.go(r);
      },
    }),
    [actions],
  );
  // A file dropped beside the composer would otherwise make the page (or
  // the app's webview) navigate to it.
  useEffect(() => {
    const guard = (e: DragEvent) => {
      if (dragHasFiles(e.dataTransfer)) e.preventDefault();
    };
    window.addEventListener("dragover", guard);
    window.addEventListener("drop", guard);
    return () => {
      window.removeEventListener("dragover", guard);
      window.removeEventListener("drop", guard);
    };
  }, []);
  return (
    <DrawerContext.Provider value={drawerCtx}>
      <div className={`app ${drawer ? "drawer-open" : ""}`} inert={navigating || undefined} style={{ "--sidebar-w": `${sidebarW}px` } as CSSProperties}>
        <Sidebar
          state={state}
          actions={sidebarActions}
          shown={visibleHosts}
          usage={<UsageStrip {...usage} />}
          open={drawer}
          hidden={sidebarHidden}
          nativeMenu={nativeMenu}
          onImport={openImport}
          onNavigate={openNavigation}
          collapseControl={
            <button ref={sidebarCollapseButton} className="btn-icon sidebar-collapse" aria-label={sidebarCollapseLabel} title={sidebarCollapseLabel} aria-controls="sidebar" aria-expanded={true} onClick={narrow ? closeDrawer : toggleSidebar}>
              <Icon d={narrow ? CLOSE : LEFT_PANEL} size={18} />
            </button>
          }
        />
        {drawer && <div className="drawer-backdrop" aria-hidden="true" onClick={closeDrawer} />}
        {sidebarHidden ? (
          <div className="sidebar-rail">
            <button ref={sidebarExpandButton} className="btn-icon" aria-label="Expand sessions sidebar" title="Expand sessions sidebar" aria-controls="sidebar" aria-expanded={false} onClick={toggleSidebar}>
              <Icon d={LEFT_PANEL} size={18} />
            </button>
          </div>
        ) : (
          <Resizer
            width={sidebarW}
            min={180}
            max={480}
            initial={SIDEBAR_W}
            dir={1}
            cssVar="--sidebar-w"
            label="Resize the sessions sidebar"
            onWidth={setSidebarW}
          />
        )}
        <div className="main" inert={drawer || undefined}>
          {!state.connected && <div className="banner">yardd is not reachable. Reconnecting.</div>}
          {state.error && (
            <div className="banner banner-error">
              <span>{state.error}</span>
              <button className="btn-text" onClick={actions.dismissError}>
                Dismiss
              </button>
            </div>
          )}
          {state.notice && !state.error && (
            <div className="banner banner-notice">
              <span>{state.notice}</span>
              <button className="btn-text" onClick={actions.dismissError}>
                Dismiss
              </button>
            </div>
          )}
          {/* The Mac app's own update, found in the background: a quiet line, never a dialog. */}
          {appUpdate.update && (
            <div className="banner banner-notice app-update" role="status">
              <span>Switchyard {appUpdate.update.version} is {appUpdate.update.ready ? "ready" : "available"}.</span>
              <span className="app-update-actions">
                <button className="btn-text" onClick={() => sendUpdateAction("notes")}>What's new</button>
                <button className="btn-text" onClick={() => sendUpdateAction(appUpdate.update!.ready ? "restart" : "notes")}>
                  {appUpdate.update.ready ? "Restart" : "Update"}
                </button>
                <button className="btn-text" onClick={appUpdate.dismiss}>Later</button>
              </span>
            </div>
          )}
          {route.page === "new" && (
            <>
              {/* The page has no bar of its own; narrow windows need one for the menu button. */}
              <header className="bar new-bar">
                <MenuButton />
                <h2 className="label-narrow">New session</h2>
                <span className="spacer" />
                <UsageStrip {...usage} compact />
                <button className="btn-icon new-connectors-shortcut" aria-label="Connectors" onClick={() => actions.go({ page: "connectors" })}><Icon d={PLUG} size={17} /></button>
                <MobileSetupButton mobile={state.mobile} hostMobile={state.hostMobile} open={() => actions.go({ page: "hosts" })} />
              </header>
              <NewSession state={state} actions={actions} onUsageAccount={setDraftUsageAccount} />
            </>
          )}
          {route.page === "agents" && <AgentsPage state={state} actions={actions} />}
          {route.page === "agent" && <AgentView state={state} actions={actions} id={route.id} />}
          {route.page === "agentEdit" &&
            // A new agent is made at once and set up in its conversation.
            (route.id === "new" ? (
              <NewAgentRedirect state={state} actions={actions} />
            ) : // Like the automation form: read once on mount, so it waits for the snapshot's list.
            state.agents.some((a) => a.id === route.id) ? (
              <AgentForm key={route.id} state={state} actions={actions} id={route.id} />
            ) : (
              <AgentsPage state={state} actions={actions} />
            ))}
          {route.page === "hosts" && <Hosts state={state} actions={actions} />}
          {route.page === "subscriptions" && <SubscriptionsPage state={state} actions={actions} />}
          {route.page === "sessions" && <SessionsPage state={state} actions={actions} />}
          {route.page === "settings" && <SettingsPage state={state} actions={actions} />}
          {route.page === "automations" && <AutomationsPage state={state} actions={actions} />}
          {route.page === "slack" && <SlackConversationPage key={`${route.team}/${route.channel}`} state={state} actions={actions} team={route.team} channel={route.channel} />}
          {route.page === "connectors" && <ConnectorsPage state={state} actions={actions} usage={<UsageStrip {...usage} compact />} />}
          {route.page === "automation" &&
            // The form reads its automation once, on mount: after a reload it waits on the list
            // until the host's snapshot has it, instead of opening blank and saving a copy.
            (route.id === "new" || state.automations[route.host]?.some((a) => a.id === route.id) ? (
              <AutomationForm key={`${route.host}/${route.id}`} state={state} actions={actions} host={route.host} id={route.id} />
            ) : (
              <AutomationsPage state={state} actions={actions} />
            ))}
          {route.page === "session" && state.sessions[route.id] && (
            <PeekScope sessionId={route.id} peek={actions.peekPr}>
              <SessionView key={route.id} meta={state.sessions[route.id]} state={state} actions={actions} />
            </PeekScope>
          )}
        </div>
      </div>
      {navigating && <NavigationPalette state={state} onPick={pickNavigation} onClose={() => setNavigating(false)} />}
      {importing && <ImportDialog state={state} actions={actions} onClose={() => setImporting(false)} />}
      <SlackIcons state={state} actions={actions} />
    </DrawerContext.Provider>
  );
}

/** First in each page's bar on narrow windows: opens the sessions drawer.
 *  Hidden where the sidebar is a column. */
export function MenuButton() {
  const { open, show } = useContext(DrawerContext);
  return (
    <button className="btn-icon menu-button" aria-label="Open sessions" aria-controls="sidebar" aria-expanded={open} onClick={(event) => show(event.currentTarget)}>
      <Icon d={MENU} size={18} />
    </button>
  );
}

/** Whether a desktop panel is collapsed, kept per browser. */
function usePanelCollapsed(key: string): [boolean, (collapsed: boolean) => void] {
  const [collapsed, setCollapsed] = useState(() => {
    try {
      return localStorage.getItem(key) === "true";
    } catch {
      return false;
    }
  });
  const set = (value: boolean) => {
    setCollapsed(value);
    try {
      localStorage.setItem(key, String(value));
    } catch {
      /* Storage can be unavailable in the webview. */
    }
  };
  return [collapsed, set];
}

/** A width the user drags, kept per browser. */
function useWidth(key: string, initial: number): [number, (w: number) => void] {
  const [width, setWidth] = useState(() => {
    try {
      const saved = Number(localStorage.getItem(key));
      return saved > 0 ? saved : initial;
    } catch {
      return initial;
    }
  });
  const set = (w: number) => {
    setWidth(w);
    try {
      localStorage.setItem(key, String(Math.round(w)));
    } catch {
      /* the width just isn't remembered */
    }
  };
  return [width, set];
}

/** A drag handle on a panel's edge. `dir` is 1 when dragging right widens the
 *  panel (a left sidebar), -1 when dragging left does (a right pane).
 *  Double-click restores the default; arrow keys nudge it. */
function Resizer(props: {
  width: number;
  min: number;
  max: number;
  initial: number;
  dir: 1 | -1;
  /** The CSS variable on the handle's parent that holds the width. */
  cssVar: string;
  label: string;
  onWidth(w: number): void;
}) {
  const { width, min, max, dir, onWidth } = props;
  const clamp = (w: number) => Math.min(Math.max(w, min), Math.max(min, max));
  const [dragging, setDragging] = useState(false);
  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const startX = e.clientX;
    const startW = width;
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    setDragging(true);
    // While dragging only the CSS variable moves, so nothing re-renders;
    // the width is kept on release.
    let w = startW;
    const move = (ev: PointerEvent) => {
      w = clamp(startW + dir * (ev.clientX - startX));
      el.parentElement?.style.setProperty(props.cssVar, `${w}px`);
    };
    const up = () => {
      onWidth(w);
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      setDragging(false);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  };
  return (
    <div
      className={`resizer resizer-${dir === 1 ? "right" : "left"} ${dragging ? "dragging" : ""}`}
      role="separator"
      aria-orientation="vertical"
      aria-label={props.label}
      aria-valuenow={Math.round(width)}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onDoubleClick={() => onWidth(props.initial)}
      onKeyDown={(e) => {
        const step = e.shiftKey ? 48 : 16;
        if (e.key === "ArrowLeft") onWidth(clamp(width - dir * step));
        else if (e.key === "ArrowRight") onWidth(clamp(width + dir * step));
        else return;
        e.preventDefault();
      }}
    />
  );
}

/** A session in the sidebar, with Pin, Archive and Delete in a menu on the
 *  row (the … button, or right-click). */
/** The row's status mark. Shapes differ as well as colors, so the states
 *  read without color too; the label next to the model says it in words. */
function RowMark({ state }: { state: RowState }) {
  return (
    <span className={`mark mark-${state}`} aria-hidden="true">
      {state === "approval" && <span className="mark-bang">!</span>}
    </span>
  );
}

/** Dragging a row onto a sidebar section moves it there; the Sidebar owns the drop side. */
interface RowDrag {
  dragging: boolean;
  start(id: string): void;
  end(): void;
}

/** An agent in the sidebar, like a direct message: its face where a
 *  session shows its harness, the same status badge, and its name. Opens
 *  its one conversation. */
function AgentRow({ state, actions, agent, active, seenAt }: { state: State; actions: Actions; agent: Agent; active: boolean; seenAt(id: string): number }) {
  const host = agentHost(agent);
  const online = host === "local" || hostOnline(state, host);
  const chat = conversationOf(agent, state.sessions);
  const status = agentStatus(agent, state.sessions, Date.now(), chat ? seenAt(chat.id) : Infinity, online ? undefined : hostName(state, host));
  // What it waits on from the owner reads as an approval does on a session row.
  const asks = !!(agent.request?.cwd || agent.request?.permission || agent.request?.slack) || !!chat?.needsYou;
  const mark: RowState = !chat ? (online ? "idle" : "offline") : asks && online ? "approval" : rowState(chat, seenAt(chat.id), online);
  // Which host it lives on, as a remote session's row says it.
  const where = host === "local" ? "" : `On ${hostName(state, host)}`;
  const detail = [agent.about, where, asks ? "Asks you for something" : status.text].filter(Boolean).join(" · ");
  return (
    <div className="session-item agent-item">
      <button
        className={`session-row agent-row ${active ? "active" : ""} ${online ? "" : "offline"} ${mark === "answered" || mark === "unread" ? "unread" : ""}`}
        title={detail}
        onClick={() => actions.go({ page: "agent", id: agent.id })}
      >
        <span className="row-engine row-agent-face">
          <AgentAvatar agent={agent} size="sm" busy={mark === "working"} />
          <RowMark state={mark} />
        </span>
        <span className="session-text">
          <span className="session-title">{agent.name}</span>
          <span className="sr-only">
            {" · "}
            <span className={`row-status row-status-${mark}`}>{asks ? "Asks you for something" : ROW_LABEL[mark]}</span>
            {where && ` · ${where}`}
          </span>
        </span>
      </button>
    </div>
  );
}

function SessionRow({ state, actions, session: s, active, groups, seenAt, unread, onUnread, onRead, drag }: { state: State; actions: Actions; session: SessionMeta; active: boolean; groups: string[]; seenAt: number; unread: boolean; onUnread(): void; onRead(): void; drag?: RowDrag }) {
  const host = hostOf(s);
  const online = host === "local" || hostOnline(state, host);
  const mark = rowState(s, seenAt, online, unread);
  const [menu, setMenu] = useState(false);
  const [confirm, setConfirm] = useState(false);
  // The menu's group pages: the list of groups, then the name field for a new one.
  const [groupPage, setGroupPage] = useState<"list" | "new" | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [handingOver, setHandingOver] = useState(false);
  const [newGroup, setNewGroup] = useState("");
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const more = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const close = () => {
    setMenu(false);
    setConfirm(false);
    setGroupPage(null);
    setNewGroup("");
    setPos(null);
  };
  useEscape(menu, close);
  // The menu lives on document.body at a fixed position. Inside the sidebar
  // the scrolling list clipped it whenever the sidebar was narrower than the
  // menu or the row sat near the list's edge. It opens under the … button,
  // flips above when there is no room below, and stays inside the window.
  useLayoutEffect(() => {
    if (!menu || !more.current || !pop.current) return;
    const at = more.current.getBoundingClientRect();
    const { width, height } = pop.current.getBoundingClientRect();
    const gap = 4;
    const edge = 8;
    const left = Math.max(edge, Math.min(at.right - width, window.innerWidth - width - edge));
    const below = at.bottom + gap;
    const above = at.top - gap - height;
    const top = below + height <= window.innerHeight - edge || above < edge ? below : above;
    setPos({ left, top: Math.max(edge, Math.min(top, window.innerHeight - height - edge)) });
  }, [menu, confirm, groupPage]);
  useEffect(() => {
    if (!menu) return;
    // pointerdown, not mousedown: the sidebar's resize handle cancels the
    // mousedown, and dragging it would strand the menu away from its row.
    const outside = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!box.current?.contains(t) && !pop.current?.contains(t)) close();
    };
    // A fixed menu would drift away from its row, so scrolling closes it.
    const scrolled = (e: Event) => !pop.current?.contains(e.target as Node) && close();
    window.addEventListener("pointerdown", outside);
    window.addEventListener("scroll", scrolled, true);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("pointerdown", outside);
      window.removeEventListener("scroll", scrolled, true);
      window.removeEventListener("resize", close);
    };
  }, [menu]);
  const rowAccount = accountLabel(accountsOn(state, host), s);
  const rowAgent = agentOf(state.agents, s);
  const rowModel = modelName(modelsOn(state, host), s.engine, s.model);
  // The row shows only the title; the rest is a hover away.
  const rowDetail = [rowAgent ? rowAgent.name : rowModel, ROW_LABEL[mark], rowAccount, host !== "local" ? `Runs on ${hostName(state, host)}` : ""].filter(Boolean).join(" · ");
  const worktree = !!(s.branch && s.repo);
  const remove = (removeWorktree: boolean) => {
    close();
    actions.remove(s.id, removeWorktree);
  };
  const moveTo = (group: string | null) => {
    close();
    actions.setGroup(s.id, group);
  };
  return (
    <div
      ref={box}
      className={`session-item ${menu ? "menu-open" : ""} ${drag?.dragging ? "dragging" : ""}`}
      // Like the menu's Move to group, only for a live row on a reachable host.
      draggable={!!drag && online && !s.archived}
      onDragStart={(e) => {
        if (!drag) return;
        e.dataTransfer.setData(SESSION_DRAG_TYPE, s.id);
        e.dataTransfer.effectAllowed = "move";
        close();
        drag.start(s.id);
      }}
      onDragEnd={() => drag?.end()}
      onContextMenu={(e) => {
        e.preventDefault();
        setMenu(true);
      }}
    >
      <button
        className={`session-row ${active ? "active" : ""} ${online ? "" : "offline"} ${mark === "answered" || mark === "unread" ? "unread" : ""}`}
        title={rowDetail}
        onClick={() => { onRead(); actions.go({ page: "session", id: s.id }); }}
      >
        {/* The harness's mark, with what the session is doing as a badge on it. */}
        <span className="row-engine">
          <EngineIcon engine={s.engine} size={16} />
          <RowMark state={mark} />
        </span>
        <span className="session-text">
          <span className="session-title">{s.title}</span>
          {/* The badge is colour and motion; screen readers get the words. */}
          <span className="sr-only">
            {" · "}
            {rowAgent ? rowAgent.name : rowModel} · <span className={`row-status row-status-${mark}`}>{ROW_LABEL[mark]}</span>
            {rowAccount && <span className="row-account"> · {rowAccount}</span>}
          </span>
        </span>
      </button>
      {/* Shown on hover: archive in one click, the rest under ⋯. */}
      <button className="btn-icon row-archive" aria-label={`${s.archived ? "Restore" : "Archive"} ${s.title}`} title={s.archived ? "Restore" : "Archive"} disabled={!online} onClick={() => actions.setArchived(s.id, !s.archived)}>
        <Icon d={ARCHIVE} size={15} />
      </button>
      <button ref={more} className="btn-icon row-more" aria-label={`Actions for ${s.title}`} aria-haspopup="menu" aria-expanded={menu} onClick={() => (menu ? close() : setMenu(true))}>
        <Icon d={MORE} size={16} />
      </button>
      {renaming && <SessionRename anchor={more} title={s.title} onSave={(title) => actions.rename(s.id, title)} onClose={() => setRenaming(false)} />}
      {handingOver && <SessionHandover anchor={more} session={s} state={state} actions={actions} onClose={() => setHandingOver(false)} />}
      {menu &&
        createPortal(
          <div ref={pop} className="row-menu" role="menu" style={pos ?? { left: 0, top: 0, visibility: "hidden" }}>
            {groupPage === "list" ? (
              <>
                <div className="row-menu-note">Move to group</div>
                {groups
                  .filter((g) => g !== s.group)
                  .map((g) => (
                    <button key={g} className="picker-item menu-item" role="menuitem" onClick={() => moveTo(g)}>
                      <Icon d={FOLDER} size={14} />
                      <span className="menu-label">{g}</span>
                    </button>
                  ))}
                <button className="picker-item menu-item" role="menuitem" onClick={() => setGroupPage("new")}>
                  <Icon d={FOLDER_PLUS} size={14} />
                  New group…
                </button>
              </>
            ) : groupPage === "new" ? (
              <form
                className="row-menu-form"
                onSubmit={(e) => {
                  e.preventDefault();
                  const name = cleanGroupName(newGroup);
                  if (name) moveTo(name);
                }}
              >
                <input
                  className="field"
                  autoFocus
                  aria-label="Group name"
                  placeholder="Group name"
                  maxLength={GROUP_NAME_MAX}
                  value={newGroup}
                  onChange={(e) => setNewGroup(e.target.value)}
                />
              </form>
            ) : !confirm ? (
              <>
                <button className="picker-item menu-item" role="menuitem" disabled={!online} onClick={() => { close(); setRenaming(true); }}>
                  <Icon d={PENCIL} size={14} />
                  Rename…
                </button>
                <button className="picker-item menu-item" role="menuitem" disabled={!online} onClick={() => { close(); setHandingOver(true); }}>
                  <Icon d="M5 5v8a3 3 0 0 0 3 3h11m-5-5 5 5-5 5" />
                  Handover…
                </button>
                <button className="picker-item menu-item" role="menuitem" onClick={() => { unread || mark === "answered" ? onRead() : onUnread(); close(); }}>
                  <Icon d={unread || mark === "answered" ? CHECK : "M4 6h16v12H4ZM4 6l8 6 8-6"} size={14} />
                  {unread || mark === "answered" ? "Mark as read" : "Mark as unread"}
                </button>
                {!s.archived && (
                  <button className="picker-item menu-item" role="menuitem" disabled={!online} onClick={() => (actions.setPinned(s.id, !s.pinned), close())}>
                    <Icon d={PIN} size={14} />
                    {s.pinned ? "Unpin" : "Pin"}
                  </button>
                )}
                {!s.archived && (
                  <button className="picker-item menu-item" role="menuitem" disabled={!online} onClick={() => setGroupPage("list")}>
                    <Icon d={FOLDER} size={14} />
                    Move to group…
                  </button>
                )}
                {!s.archived && s.group && (
                  <button className="picker-item menu-item" role="menuitem" disabled={!online} onClick={() => moveTo(null)}>
                    <Icon d={CLOSE} size={14} />
                    Remove from group
                  </button>
                )}
                <button className="picker-item menu-item" role="menuitem" disabled={!online} onClick={() => (actions.setArchived(s.id, !s.archived), close())}>
                  <Icon d={ARCHIVE} size={14} />
                  {s.archived ? "Restore" : "Archive"}
                </button>
                <div className="menu-divider" />
                <button className="picker-item menu-item danger" role="menuitem" disabled={!online} onClick={() => setConfirm(true)}>
                  <Icon d={TRASH} size={14} />
                  Delete…
                </button>
              </>
            ) : (
              <>
                <div className="row-menu-note">
                  {s.status === "running" || s.status === "needs_approval" ? "It is still working; deleting stops it. " : ""}
                  This can't be undone.
                </div>
                <button className="picker-item menu-item danger" role="menuitem" onClick={() => remove(false)}>
                  {worktree ? "Delete, keep the worktree" : "Delete session"}
                </button>
                {worktree && (
                  <button className="picker-item menu-item danger" role="menuitem" onClick={() => remove(true)}>
                    Delete and remove the worktree
                  </button>
                )}
                <button className="picker-item menu-item" role="menuitem" onClick={() => setConfirm(false)}>
                  Cancel
                </button>
              </>
            )}
          </div>,
          document.body,
        )}
    </div>
  );
}

/** How long a drag rests on a collapsed group before it opens. */
const SPRING_OPEN_MS = 600;

/** Whether a media query matches, following changes. */
function useMedia(query: string): boolean {
  const [matches, setMatches] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const mq = window.matchMedia(query);
    const onChange = () => setMatches(mq.matches);
    onChange();
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [query]);
  return matches;
}

/** Which groups the user folded in this browser. */
const COLLAPSED_KEY = "yard.groups.collapsed";
function useCollapsedGroups(): [Set<string>, (name: string) => void] {
  const [collapsed, setCollapsed] = useState<Set<string>>(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? "[]");
      return new Set(Array.isArray(saved) ? saved.filter((n) => typeof n === "string") : []);
    } catch {
      return new Set();
    }
  });
  const toggle = (name: string) =>
    setCollapsed((was) => {
      const next = new Set(was);
      if (!next.delete(name)) next.add(name);
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify([...next]));
      return next;
    });
  return [collapsed, toggle];
}

/** A group's header: the name, how many sessions, a chevron; its menu renames
 *  or ungroups every session in it, archived ones included. Collapsed, it
 *  hides its rows' marks, so it carries the Automations row's dot instead:
 *  a group folded days ago must not hide an approval. */
function GroupHead({ name, count, open, onToggle, members, mark, actions }: { name: string; count: number; open: boolean; onToggle(): void; members: SessionMeta[]; mark: "approval" | "working" | null; actions: Actions }) {
  const [menu, setMenu] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState(name);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const more = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const close = () => {
    setMenu(false);
    setPos(null);
  };
  useEscape(menu, close);
  // Leaving the field by Escape or Enter must not commit a second time through blur.
  const done = useRef(false);
  useLayoutEffect(() => {
    if (!menu || !more.current || !pop.current) return;
    const at = more.current.getBoundingClientRect();
    const { width, height } = pop.current.getBoundingClientRect();
    const edge = 8;
    const left = Math.max(edge, Math.min(at.right - width, window.innerWidth - width - edge));
    const below = at.bottom + 4;
    const above = at.top - 4 - height;
    const top = below + height <= window.innerHeight - edge || above < edge ? below : above;
    setPos({ left, top: Math.max(edge, Math.min(top, window.innerHeight - height - edge)) });
  }, [menu]);
  useEffect(() => {
    if (!menu) return;
    const outside = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!more.current?.contains(t) && !pop.current?.contains(t)) close();
    };
    const scrolled = (e: Event) => !pop.current?.contains(e.target as Node) && close();
    window.addEventListener("pointerdown", outside);
    window.addEventListener("scroll", scrolled, true);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("pointerdown", outside);
      window.removeEventListener("scroll", scrolled, true);
      window.removeEventListener("resize", close);
    };
  }, [menu]);
  const rename = () => {
    if (done.current) return;
    done.current = true;
    const next = cleanGroupName(draft);
    setRenaming(false);
    if (!next || next === name) return;
    for (const s of members) actions.setGroup(s.id, next);
  };
  return (
    <div className={`group-head session-item ${menu ? "menu-open" : ""}`}>
      {renaming ? (
        <input
          className="field group-rename"
          autoFocus
          aria-label="Group name"
          maxLength={GROUP_NAME_MAX}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onFocus={(e) => e.target.select()}
          onBlur={rename}
          onKeyDown={(e) => {
            if (e.key === "Enter") (e.preventDefault(), rename());
            else if (e.key === "Escape") (done.current = true, setRenaming(false));
          }}
        />
      ) : (
        <button className="list-head list-toggle group-toggle" aria-expanded={open} onClick={onToggle}>
          <span className={`tool-chevron ${open ? "open" : ""}`}>
            <Icon d={CHEVRON_RIGHT} size={11} />
          </span>
          <span className="group-name">{name}</span>
          <span className="muted">{count}</span>
          {!open && mark && <span className={`dot ${mark === "approval" ? "dot-needs_approval" : "dot-running"}`} role="img" aria-label={GROUP_MARK[mark]} title={GROUP_MARK[mark]} />}
        </button>
      )}
      {!renaming && (
        <button ref={more} className="btn-icon row-more" aria-label={`Actions for group ${name}`} aria-haspopup="menu" aria-expanded={menu} onClick={() => (menu ? close() : setMenu(true))}>
          <Icon d={MORE} size={16} />
        </button>
      )}
      {menu &&
        createPortal(
          <div ref={pop} className="row-menu" role="menu" style={pos ?? { left: 0, top: 0, visibility: "hidden" }}>
            <button
              className="picker-item menu-item"
              role="menuitem"
              onClick={() => {
                close();
                setDraft(name);
                done.current = false;
                setRenaming(true);
              }}
            >
              <Icon d={PENCIL} size={14} />
              Rename
            </button>
            <button
              className="picker-item menu-item"
              role="menuitem"
              onClick={() => {
                close();
                for (const s of members) actions.setGroup(s.id, null);
              }}
            >
              <Icon d={CLOSE} size={14} />
              Ungroup
            </button>
          </div>,
          document.body,
        )}
    </div>
  );
}

function Sidebar({ state, actions, shown, usage, open, hidden, nativeMenu, onImport, onNavigate, collapseControl }: {
  state: State;
  actions: Actions;
  shown: string[];
  usage: ReactNode;
  open: boolean;
  hidden: boolean;
  nativeMenu: boolean;
  onImport(): void;
  onNavigate(): void;
  collapseControl: ReactNode;
}) {
  // Runs of automations live on the Automations page, and agents'
  // conversations on the Agents page, not in this list.
  const everything = Object.values(state.sessions);
  const slackSessions = new Set(state.slack?.places?.map((p) => p.sessionId));
  const all = everything.filter((s) => !hiddenFromSidebar(s) && !isAgentConversation(s, state.agents) && !slackSessions.has(s.id));
  const { pinned, ungrouped: active, groups, archived } = sidebarLayout(all);
  // Every group a session can be moved to, including ones whose only live
  // members are pinned.
  const groupNames = [...new Set(all.filter((s) => s.group && !s.archived).map((s) => s.group as string))].sort((a, b) => a.localeCompare(b));
  const [collapsed, toggleGroup] = useCollapsedGroups();
  const { route } = state;
  const narrow = useMedia(NARROW);
  // The row being dragged. The ref answers dragover at once; the state (the
  // dimmed row, the empty Sessions slot) renders a tick later, because
  // changing the page inside dragstart can cancel the drag or tint its image.
  const dragRef = useRef<string | null>(null);
  const [dragId, setDragId] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const endDrag = () => {
    dragRef.current = null;
    setDragId(null);
    setOver(null);
  };
  const rowDrag = (id: string): RowDrag => ({
    dragging: dragId === id,
    start: (sid) => {
      dragRef.current = sid;
      setTimeout(() => dragRef.current === sid && setDragId(sid), 0);
    },
    end: endDrag,
  });
  // A section takes a drop only when it would change something; elsewhere the
  // cursor says no and nothing lights up.
  const dropZone = (key: string, target: DropTarget) => ({
    className: `drop-zone ${over === key ? "drop-over" : ""}`,
    onDragOver: (e: ReactDragEvent<HTMLDivElement>) => {
      if (!dragHasSession(e.dataTransfer)) return;
      const s = dragRef.current ? state.sessions[dragRef.current] : undefined;
      if (!s || !dropUpdate(s, target)) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      if (over !== key) setOver(key);
    },
    onDragLeave: (e: ReactDragEvent<HTMLDivElement>) => {
      if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setOver((o) => (o === key ? null : o));
    },
    onDrop: (e: ReactDragEvent<HTMLDivElement>) => {
      const id = e.dataTransfer.getData(SESSION_DRAG_TYPE);
      const s = state.sessions[id];
      const update = s && dropUpdate(s, target);
      endDrag();
      if (!update) return;
      e.preventDefault();
      actions.place(id, update);
    },
  });
  // A collapsed group opens after the drag rests on it for a moment.
  const overGroup = over?.startsWith("group:") ? over.slice(6) : null;
  useEffect(() => {
    if (overGroup === null || !collapsed.has(overGroup)) return;
    const t = setTimeout(() => toggleGroup(overGroup), SPRING_OPEN_MS);
    return () => clearTimeout(t);
  }, [overGroup, collapsed]);
  const dragged = dragId ? state.sessions[dragId] : undefined;
  // Runs of automations live on the Automations page, not in this list.
  const runs = everything.filter(hiddenFromSidebar);
  const runsMark = runsAttention(runs);
  const archivedMark = runsAttention(archived);
  const inRun = route.page === "session" && !!state.sessions[route.id]?.automation;
  const agentOpen = route.page === "agent" ? state.agents.find((a) => a.id === route.id)?.chat ?? null : null;
  const seen = useSeen(state.sessions, route.page === "session" ? route.id : agentOpen, state.restore);
  // The Agents row carries what the sidebar would show for their conversations.
  const slackWaiting = state.slack?.places?.some((p) => {
    const session = state.sessions[p.sessionId];
    return session?.status === "needs_approval" && hostOnline(state, hostOf(session));
  });
  const agentsMark = slackWaiting ? "approval" : agentsAttention(state.agents, state.sessions, seen.seenAt);
  const inAgent = route.page === "session" && !!state.sessions[route.id] && (slackSessions.has(route.id) || isAgentConversation(state.sessions[route.id], state.agents));
  // Going to an archived session shows the section, so its row stays in
  // view. Only on going there: archiving the session already open leaves the
  // section as it was (it used to spring open on every archive).
  const openId = route.page === "session" ? route.id : null;
  const openArchived = !!openId && !!state.sessions[openId]?.archived;
  const [showArchived, setShowArchived] = useState(openArchived);
  useEffect(() => {
    if (openArchived) setShowArchived(true);
  }, [openId]);
  // Opened as a drawer, focus moves into it, so the keyboard is where the eyes are.
  const nav = useRef<HTMLElement>(null);
  useEffect(() => {
    if (open) nav.current?.querySelector<HTMLElement>(".nav-row")?.focus();
  }, [open]);
  const row = (s: SessionMeta) => (
    <SessionRow
      key={s.id}
      state={state}
      actions={actions}
      session={s}
      groups={groupNames}
      active={route.page === "session" && route.id === s.id}
      seenAt={seen.seenAt(s.id)}
      unread={seen.isUnread(s.id)}
      onUnread={() => seen.markUnread(s.id)}
      onRead={() => seen.markRead(s.id)}
      // Touch has no HTML5 drag; in the phone drawer the row menu moves sessions.
      drag={narrow ? undefined : rowDrag(s.id)}
    />
  );
  const shownById = new Map(all.map((session) => [session.id, session]));
  const children = new Map<string, SessionMeta[]>();
  const nestedIds = new Set<string>();
  // A hand-off nests only inside its own section: under a pinned parent an
  // unpinned child would leave its group's heading counting a row it lacks.
  for (const session of [...active, ...groups.flatMap((group) => group.sessions), ...archived]) {
    const parent = session.parent ? shownById.get(session.parent) : undefined;
    if (!parent || parent.id === session.id || parent.createdAt > session.createdAt ||
        !!parent.archived !== !!session.archived || !!parent.pinned !== !!session.pinned ||
        (parent.group ?? "") !== (session.group ?? "")) continue;
    const siblings = children.get(parent.id) ?? [];
    siblings.push(session);
    children.set(parent.id, siblings);
    nestedIds.add(session.id);
  }
  const tree = (session: SessionMeta): ReactNode => {
    const subsessions = children.get(session.id);
    return subsessions?.length ? (
      <Fragment key={session.id}>
        {row(session)}
        <div className="session-children" role="group" aria-label={`Handovers from ${session.title}`}>
          {subsessions.map(tree)}
        </div>
      </Fragment>
    ) : row(session);
  };
  const roots = (sessions: SessionMeta[]) => sessions.filter((session) => !nestedIds.has(session.id));
  // With no ungrouped rows there is nothing to drop on to leave a group, so a slot stands in.
  const ungroupSlot = active.length === 0 && !!dragged && !!dropUpdate(dragged, { kind: "ungrouped" });
  return (
    <nav ref={nav} id="sidebar" className="sidebar" aria-label="Sessions" hidden={hidden}>
      <div className="sidebar-heading">
        <button className="brand btn-text" onClick={onNavigate} aria-label="Go to session or page" aria-keyshortcuts={/Mac|iPhone|iPad/.test(navigator.platform) ? "Meta+K" : "Control+K"}>
          Switchyard <kbd>{/Mac|iPhone|iPad/.test(navigator.platform) ? "⌘K" : "Ctrl K"}</kbd>
        </button>
        {collapseControl}
      </div>
      <div className="sidebar-top">
        <button className={`nav-row ${route.page === "new" ? "active" : ""}`} onClick={() => actions.go({ page: "new" })}>
          <Icon d={PLUS} />
          New session
        </button>
        <button aria-label="Agents" className={`nav-row ${route.page === "agents" || route.page === "agent" || route.page === "agentEdit" || inAgent ? "active" : ""}`} onClick={() => actions.go({ page: "agents" })}>
          <Icon d={AT} />
          Agents
          {agentsMark && <span className={`dot dot-agents dot-${agentsMark}`} role="img" aria-label={AGENTS_MARK[agentsMark]} title={AGENTS_MARK[agentsMark]} />}
        </button>
        <button className={`nav-row ${route.page === "automations" || route.page === "automation" || inRun ? "active" : ""}`} onClick={() => actions.go({ page: "automations" })}>
          <Icon d={CLOCK} />
          Automations
          {runsMark && <span className={`dot ${runsMark === "approval" ? "dot-needs_approval" : "dot-running"}`} role="img" aria-label={RUNS_MARK[runsMark]} title={RUNS_MARK[runsMark]} />}
        </button>
        <button className={`nav-row ${route.page === "connectors" ? "active" : ""}`} onClick={() => actions.go({ page: "connectors" })}>
          <Icon d={PLUG} />
          Connectors
        </button>
        {!nativeMenu && (
          <>
            <button className="nav-row" onClick={onImport}>
              <Icon d={IMPORT} />
              Import
            </button>
            <button className={`nav-row ${route.page === "settings" ? "active" : ""}`} onClick={() => actions.go({ page: "settings" })}>
              <Icon d={GEAR} />
              Settings
            </button>
          </>
        )}
      </div>
      <div className="session-list">
        {pinned.length > 0 && (
          <div {...dropZone("pinned", { kind: "pinned" })}>
            <div className="list-head">Pinned</div>
            {pinned.map(tree)}
          </div>
        )}
        {(active.length > 0 || ungroupSlot) && (
          <div {...dropZone("ungrouped", { kind: "ungrouped" })}>
            {pinned.length > 0 && active.length > 0 && <div className="list-head">Sessions</div>}
            {roots(active).map(tree)}
            {ungroupSlot && <div className="drop-slot">{dragged?.group ? "Drop here to remove from group" : "Drop here to unpin"}</div>}
          </div>
        )}
        {all.length === 0 && <div className="empty">No sessions yet.</div>}
        {groups.map((g) => {
          const zone = dropZone(`group:${g.name}`, { kind: "group", name: g.name });
          return (
            <div key={g.name} {...zone} className={`${zone.className} group-zone`}>
              <GroupHead
                name={g.name}
                count={g.sessions.length}
                open={!collapsed.has(g.name)}
                onToggle={() => toggleGroup(g.name)}
                members={all.filter((s) => s.group === g.name)}
                mark={runsAttention(g.sessions)}
                actions={actions}
              />
              {!collapsed.has(g.name) && roots(g.sessions).map(tree)}
            </div>
          );
        })}
        {/* The owner's own Slack: his direct messages and group DMs. */}
        <SlackSidebar state={state} actions={actions} route={route} />
        {/* Agents, like direct messages: the list's last section. */}
        {state.agents.length > 0 && (
          <div className="agent-dms" role="group" aria-label="Agents">
            <div className="list-head">Agents</div>
            {state.agents.map((a) => (
              <AgentRow
                key={a.id}
                state={state}
                actions={actions}
                agent={a}
                active={(route.page === "agent" && route.id === a.id) || (route.page === "session" && (route.id === a.chat || !!a.previous?.includes(route.id)))}
                seenAt={seen.seenAt}
              />
            ))}
          </div>
        )}
      </div>
      {archived.length > 0 && (
        <div className="archived-dock">
          <button className="list-head list-toggle" aria-expanded={showArchived} onClick={() => setShowArchived(!showArchived)}>
            <span>Archived</span>
            <span className="muted">{archived.length}</span>
            {/* Archived sessions still run: collapsed, this dot is the only sign one waits for the user. */}
            {!showArchived && archivedMark && (
              <span className={`dot ${archivedMark === "approval" ? "dot-needs_approval" : "dot-running"}`} role="img" aria-label={GROUP_MARK[archivedMark]} title={GROUP_MARK[archivedMark]} />
            )}
            <span className={`tool-chevron ${showArchived ? "open" : ""}`}>
              <Icon d={CHEVRON_RIGHT} size={11} />
            </span>
          </button>
          {showArchived && <div className="archived-list">{roots(archived).map(tree)}</div>}
        </div>
      )}
      {usage}
      {shown.length > 0 && (
        <div className="sidebar-bottom">
          {shown.map((id) => {
            const stats = state.host[id];
            const status = hostState(state, id);
            return (
              <button key={id} className="host-row" onClick={() => actions.go({ page: "hosts", host: id })} title={HOST_STATE_LABEL[status]}>
                <span className={`dot host-dot-${status}`} aria-hidden="true" />
                <span className="host-name">{hostName(state, id)}</span>
                {id !== "local" && <Cloud size={12} />}
                <span className="spacer" />
                <span className="host-stat">
                  {(status === "ok" || status === "busy") && stats
                    ? `${pct(stats.cpu)} · ${pct((stats.memUsed / stats.memTotal) * 100)}`
                    : HOST_STATE_LABEL[status]}
                </span>
              </button>
            );
          })}
        </div>
      )}
    </nav>
  );
}

/** yardd's own verdict (the one its queue uses) when it sends one. */
const hostPressure = (h: Host) =>
  h.pressure !== undefined ? !!h.pressure : h.load > h.cores || h.memUsed / h.memTotal > 0.9;

/* ---------- model picker ---------- */

export function effortFor(model: ModelInfo | undefined, wanted: string): string {
  if (!model) return wanted;
  if (model.efforts.includes(wanted)) return wanted;
  return model.defaultEffort ?? model.efforts[0] ?? wanted;
}

/** What the engine may do without asking. Changing it mid-session applies
 *  to Claude at once and to Codex from its next turn. */
export function PermissionPicker({ engine, value, onChange }: { engine: Engine; value: Permission; onChange(p: Permission): void }) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  const current = PERMISSIONS.find((p) => p.id === value) ?? PERMISSIONS[0];
  return (
    <div className="picker">
      <button
        ref={anchor}
        className={`btn-quiet permission ${value === "full" ? "permission-full" : ""}`}
        aria-haspopup="true"
        aria-expanded={open}
        title={current[engine]}
        onClick={() => setOpen(!open)}
      >
        <span>{current.label}</span>
        <span className="muted">
          <Icon d={CHEVRON_DOWN} size={12} />
        </span>
      </button>
      {open && (
        <AnchoredPopover anchor={anchor} label="Permissions" onClose={() => setOpen(false)}>
          <div className="popover-heading"><strong>Permissions</strong></div>
          <div className="permission-menu choice-menu" role="menu">
            {PERMISSIONS.map((p) => (
              <button
                key={p.id}
                role="menuitemradio"
                aria-checked={p.id === value}
                className={`picker-item permission-item ${p.id === value ? "active" : ""}`}
                onClick={() => {
                  onChange(p.id);
                  setOpen(false);
                }}
              >
                <span className="permission-label">{p.label}</span>
                <span className="muted small">{p[engine]}</span>
              </button>
            ))}
          </div>
        </AnchoredPopover>
      )}
    </div>
  );
}

/** Effort for the chosen model, its own chip next to the model's. A model
 *  without efforts (some OpenCode and Grok ones) runs as it is: no chip. */
const EFFORT_DESCRIPTIONS: Record<string, string> = {
  minimal: "Responds quickly with little planning",
  low: "Suited to small edits and quick questions",
  medium: "Balanced for everyday tasks",
  high: "Thinks longer about tricky changes",
  xhigh: "More reasoning for complex tasks",
  max: "Deep reasoning for the hardest work",
  ultra: "The most reasoning this model offers",
};

export function EffortPicker({ models, value, onChange }: { models: Models; value: Choice; onChange(effort: string): void }) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  const efforts = models[value.engine]?.find((m) => m.id === value.model)?.efforts ?? [];
  if (efforts.length === 0) return null;
  return (
    <div className="picker">
      <button ref={anchor} className="btn-quiet" aria-haspopup="true" aria-expanded={open} aria-label={`Effort ${capitalize(value.effort)}`} onClick={() => setOpen(!open)}>
        <span>{capitalize(value.effort)}</span>
        <span className="muted">
          <Icon d={CHEVRON_DOWN} size={12} />
        </span>
      </button>
      {open && (
        <AnchoredPopover anchor={anchor} label="Effort" onClose={() => setOpen(false)}>
          <div className="popover-heading"><strong>Effort</strong></div>
          <div className="effort-menu choice-menu" role="menu" aria-label="Effort">
            {efforts.map((e) => (
              <button
                key={e}
                role="menuitemradio"
                aria-checked={e === value.effort}
                className={`picker-item ${e === value.effort ? "active" : ""}`}
                onClick={() => {
                  onChange(e);
                  setOpen(false);
                }}
              >
                <span>{capitalize(e)}</span>
                {EFFORT_DESCRIPTIONS[e] && <small className="muted">{EFFORT_DESCRIPTIONS[e]}</small>}
                {e === value.effort && <Icon d={CHECK} size={13} />}
              </button>
            ))}
          </div>
        </AnchoredPopover>
      )}
    </div>
  );
}

export function AccountPicker({
  state,
  host,
  engine,
  account,
  engines,
  onPick,
  onManage,
}: {
  state: State;
  host: string;
  engine: Engine;
  account: string;
  /** The engines whose accounts may be picked; every engine the host lists when left out. */
  engines?: Engine[];
  onPick?(engine: Engine, account: string): void;
  onManage(): void;
}) {
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const accounts = accountsOn(state, host);
  const name = accountName(accounts, engine, account);
  const states = state.engines[host];
  const login = (e: Engine, id: string) => (id === DEFAULT_ACCOUNT ? states?.[e] : states?.accounts?.[id]);
  const current = login(engine, account);
  return (
    <div className="picker account-picker">
      <button
        ref={anchor}
        className="btn-quiet account-trigger"
        aria-label={`Account: ${ENGINE_NAME[engine]}, ${name}`}
        title={`${ENGINE_NAME[engine]} · ${name}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <span className="account-avatar" aria-hidden="true">
          {name.trim().split(/\s+/).slice(0, 2).map((word) => word[0]?.toUpperCase()).join("")}
        </span>
        <span className="account-trigger-name">
          {ENGINE_NAME[engine]} · {name}
        </span>
        <Icon d={CHEVRON_DOWN} size={12} />
      </button>
      {open && (
        <AnchoredPopover anchor={anchor} label="Engine account identity" onClose={() => setOpen(false)}>
          <div className="popover-heading">
            <strong>{onPick ? "Choose an account" : "This session runs as"}</strong>
            <button className="btn-icon" aria-label="Close account picker" onClick={() => setOpen(false)}>
              ×
            </button>
          </div>
          {onPick ? (
            <div role="menu" aria-label="Engine accounts">
              {(engines ?? pickerEngines(modelsOn(state, host), engine)).flatMap((e) =>
                accountChoices(accounts, e).map((a) => (
                  <button
                    key={`${e}:${a.id}`}
                    className="btn-quiet account-option"
                    role="menuitemradio"
                    aria-checked={e === engine && a.id === account}
                    disabled={!modelsOn(state, host)[e]?.length}
                    onClick={() => {
                      onPick(e, a.id);
                      setOpen(false);
                    }}
                  >
                    <span>
                      <strong>
                        {ENGINE_NAME[e]} · {a.name}
                      </strong>
                      <small>{login(e, a.id)?.account || (login(e, a.id) ? stateText(login(e, a.id)!) : "Checking login…")}</small>
                    </span>
                    {e === engine && a.id === account && <span aria-hidden="true">✓</span>}
                  </button>
                )),
              )}
            </div>
          ) : (
            <div className="account-detail">
              <strong>
                {ENGINE_NAME[engine]} · {name}
              </strong>
              <p className="muted">{current?.account || (current ? stateText(current) : "Login status unavailable")}</p>
              <span className="account-host-badge">{hostName(state, host)}</span>
            </div>
          )}
          <div className="popover-footer">
            <span className="muted small">{onPick ? hostName(state, host) : "Fixed for this session"}</span>
            <button
              className="btn-text"
              onClick={() => {
                setOpen(false);
                onManage();
              }}
            >
              {onPick ? "Manage accounts →" : "New session →"}
            </button>
          </div>
        </AnchoredPopover>
      )}
    </div>
  );
}

const STANDARD_SPEED = { id: "default", name: "Standard", description: "Default speed" };

function SpeedOptions({ options, selected, onChange, onBack, nextTurn }: {
  options: NonNullable<ModelInfo["serviceTiers"]>;
  selected: string;
  onChange(value: string): void;
  onBack(): void;
  nextTurn?: boolean;
}) {
  return (
    <>
      <div className="popover-heading speed-heading">
        <button className="btn-icon" aria-label="Back to models" onClick={onBack}><Icon d="M15 18l-6-6 6-6" /></button>
        <strong>Codex speed</strong>
      </div>
      {nextTurn && <p className="muted small speed-note">Applies to the next turn.</p>}
      <div className="effort-menu choice-menu" role="menu" aria-label="Codex speed">
        {options.map((tier) => (
          <button
            key={tier.id}
            className="picker-item"
            role="menuitemradio"
            autoFocus={tier.id === selected}
            aria-checked={tier.id === selected}
            onClick={() => onChange(tier.id)}
          >
            <span>{tier.name}</span>
            {tier.description && <small className="muted">{tier.description}</small>}
            {tier.id === selected && <Icon d={CHECK} size={13} />}
          </button>
        ))}
      </div>
    </>
  );
}

export function ModelPicker(props: {
  models: Models;
  value: Choice;
  engines: Engine[];
  onChange(next: Choice): void;
  speed?: { value: string; onChange(value: string): void; nextTurn?: boolean };
  /** In a session, a model of another engine means a hand-off, never a silent switch. */
  handoff?: { blocked: boolean; onConfirm(next: Choice): void };
}) {
  const { models, value, engines, onChange, handoff } = props;
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  const speedTrigger = useRef<HTMLButtonElement>(null);
  const [showSpeed, setShowSpeed] = useState(false);
  const [confirm, setConfirm] = useState<Choice | null>(null);
  // Engines not in use start folded, each opened on its own.
  const [expanded, setExpanded] = useState<Engine[]>([]);
  // OpenCode can list a hundred models across its providers.
  const [filter, setFilter] = useState("");
  const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
  const ordered = [value.engine, ...engines.filter((e) => e !== value.engine)];
  const popover = useRef<HTMLDivElement>(null);
  const selectedRow = useRef<HTMLButtonElement>(null);
  const current = models[value.engine]?.find((m) => m.id === value.model);
  const speedOptions = value.engine === "codex" && props.speed && current?.serviceTiers?.length
    ? [STANDARD_SPEED, ...current.serviceTiers.filter((tier) => tier.id !== STANDARD_SPEED.id)] : [];
  const selectedSpeed = speedOptions.find((tier) => tier.id === props.speed?.value) ?? STANDARD_SPEED;
  const speedOpen = showSpeed && speedOptions.length > 0;
  useLayoutEffect(() => {
    if (!open || speedOpen) return;
    // The hand-off confirmation is at the top, above the model just selected.
    if (confirm) popover.current?.scrollTo({ top: 0 });
    else selectedRow.current?.scrollIntoView({ block: "nearest" });
  }, [open, speedOpen, confirm]);
  const backToModel = () => {
    setShowSpeed(false);
    requestAnimationFrame(() => speedTrigger.current?.focus({ preventScroll: true }));
  };
  const close = () => {
    setShowSpeed(false);
    setOpen(false);
    setConfirm(null);
    setExpanded([]);
    setFilter("");
  };
  return (
    <div className="picker">
      <button
        ref={anchor}
        className="btn-quiet model-trigger"
        aria-haspopup="true"
        aria-expanded={open}
        aria-label={`Model ${current?.name ?? value.model}`}
        onClick={() => (open ? close() : setOpen(true))}
      >
        <span className="model-provider" aria-hidden="true">
          <EngineIcon engine={value.engine} size={14} />
        </span>
        <span>{current?.name ?? value.model}</span>
        <span className="muted">
          <Icon d={CHEVRON_DOWN} size={12} />
        </span>
      </button>
      {open && (
        <AnchoredPopover anchor={anchor} label={speedOpen ? "Codex speed" : "Model"} variant={speedOpen ? "speed-panel" : "model-panel"} onClose={close} onEscape={speedOpen ? backToModel : undefined}>
          {speedOpen ? (
            <SpeedOptions
              options={speedOptions}
              selected={selectedSpeed.id}
              nextTurn={props.speed?.nextTurn}
              onBack={backToModel}
              onChange={(tier) => { props.speed?.onChange(tier); backToModel(); }}
            />
          ) : (
            <>
              <div className="popover-heading">
                <strong>Model</strong>
                {speedOptions.length > 0 && (
                  <button ref={speedTrigger} className="btn-quiet speed-trigger" aria-label={`Speed ${selectedSpeed.name}`} onClick={() => setShowSpeed(true)}>
                    <Icon d="m13 2-9 12h7l-1 8 10-12h-7l1-8Z" size={14} />
                    {selectedSpeed.name}
                    <Icon d={CHEVRON_RIGHT} size={12} />
                  </button>
                )}
              </div>
              <div className="model-popover" ref={popover}>
                {confirm && handoff && (
                  <div className="handoff-confirm" role="group" aria-label="Confirm hand-off">
                    <div>
                      <strong>Hand off to {ENGINE_NAME[confirm.engine]}</strong>: starts a new session in this folder with a summary of this one.
                    </div>
                    <div className="muted small">
                      {modelName(models, confirm.engine, confirm.model)} · {capitalize(confirm.effort)}
                    </div>
                    <div className="row">
                      <button
                        className="btn-primary"
                        disabled={handoff.blocked}
                        onClick={() => {
                          handoff.onConfirm(confirm);
                          close();
                        }}
                      >
                        Hand off
                      </button>
                      <button className="btn-text" onClick={() => setConfirm(null)}>
                        Cancel
                      </button>
                    </div>
                  </div>
                )}
                {ordered.map((engine) => {
                  const isHandoff = !!handoff && engine !== value.engine;
                  const disabled = isHandoff && handoff.blocked;
                  const list = models[engine] ?? [];
                  // The engine in use is open; each other one is a row until asked for.
                  const folded = engine !== value.engine && !expanded.includes(engine);
                  return (
                    <div key={engine} className="picker-group">
                      {folded ? (
                        <button className="btn-text picker-fold" aria-expanded={false} onClick={() => setExpanded([...expanded, engine])}>
                          <span>
                            {`${ENGINE_NAME[engine]} · ${list.length} ${list.length === 1 ? "model" : "models"}`}
                            {handoff ? " · switching hands off" : ""}
                          </span>
                          <span className="muted">
                            <Icon d={CHEVRON_RIGHT} size={12} />
                          </span>
                        </button>
                      ) : (
                        <>
                          <div className="picker-head">
                            <span>{ENGINE_NAME[engine]}</span>
                          </div>
                          {isHandoff && (
                            <div className="picker-note">{disabled ? "Stop the running turn to hand off." : "Picking one hands off to a new session."}</div>
                          )}
                          {list.length === 0 && <div className="picker-empty">Not available</div>}
                          {list.length > 12 && (
                            <input
                              className="picker-filter"
                              value={filter}
                              onChange={(e) => setFilter(e.target.value)}
                              placeholder={`Filter ${ENGINE_NAME[engine]} models`}
                              aria-label={`Filter ${ENGINE_NAME[engine]} models`}
                              spellCheck={false}
                            />
                          )}
                          {(list.length > 12 ? list.filter((m) => words.every((w) => `${m.name} ${m.id}`.toLowerCase().includes(w))) : list).map((m) => {
                            const isCurrent = engine === value.engine && m.id === value.model;
                            return (
                              <button
                                key={m.id}
                                ref={isCurrent ? selectedRow : undefined}
                                className={`picker-item picker-model ${isCurrent ? "active" : ""} ${
                                  confirm?.engine === engine && confirm.model === m.id ? "active" : ""
                                }`}
                                disabled={disabled}
                                aria-current={isCurrent ? "true" : undefined}
                                onClick={() => {
                                  const next = { engine, model: m.id, effort: effortFor(m, value.effort) };
                                  if (isHandoff) {
                                    setConfirm(next);
                                    return;
                                  }
                                  onChange(next);
                                  close();
                                }}
                              >
                                <span>{m.name}</span>
                                {isCurrent && <Icon d={CHECK} size={13} />}
                              </button>
                            );
                          })}
                        </>
                      )}
                    </div>
                  );
                })}
              </div>
            </>
          )}
        </AnchoredPopover>
      )}
    </div>
  );
}

/* ---------- composer ---------- */

/** A file in the composer, uploaded as soon as it is added so sending is quick. */
interface PendingFile {
  key: string;
  name: string;
  size: number;
  /** Absent for a file already on the host (a queued message given back by
   *  Edit or Stop): it goes again by id, with nothing to upload. */
  file?: File;
  /** Object URL for a local preview of an image. */
  preview?: string;
  state: "uploading" | "ready" | "error";
  error?: string;
}

type Upload = { target: string; promise: Promise<Attachment> };
/** What a session's reply box keeps while another session is open (compose.ts). */
type ComposerDraft = {
  text: string;
  quotes: (Quote & { key: string })[];
  snippets: (Snippet & { key: string })[];
  files: PendingFile[];
  uploads: Map<string, Upload>;
};

export function Composer(props: {
  placeholder: string;
  rows: number;
  large?: boolean;
  busy?: boolean;
  /** A turn is running: sending queues the message. Says when the agent gets it (see queuedHint). */
  queueing?: string;
  /** Up arrow in an empty box: take the last queued message back to edit. True when it did. */
  onEditLastQueued?(): boolean;
  /** Keep the text and files after sending, for a submit that can still fail. */
  keepText?: boolean;
  /** Session whose terminal can add text here (see compose.ts). */
  appendFor?: string;
  /** Key the box's draft is kept under while it is not shown; defaults to appendFor. */
  draft?: string;
  /** Where attached files are stored: the host the prompt will run on. */
  target: UploadTarget;
  /** Omit for text-only endpoints; the shared attachment control is disabled. */
  upload?(file: File, target: UploadTarget): Promise<Attachment>;
  maxLength?: number;
  /** False leaves the draft in place when the endpoint rejects a submission. */
  onSubmit(text: string, attachments: Attachment[]): void | boolean | Promise<void | boolean>;
  /** Hands-free voice mode for a session: what it has to say, numbered. */
  voice?: { scope: string; cue: { n: number; cue: Cue } | null; approval: boolean; carry?: boolean };
  settings?: { label: string; model: string; effort: string; full: boolean };
  /** What the narrow box's setup button and popover are called: "Session setup" unless an agent's. */
  setupTitle?: string;
  /** Agents an `@` offers. A message starting with one goes to it (the caller routes it). */
  mentions?: Agent[];
  /** What a leading `@Name` does with this message, shown above the box; null says nothing. */
  mentionHint?(agent: Agent): string | null;
  children: ReactNode;
}) {
  const toolbar = useRef<HTMLDivElement>(null);
  const setup = useRef<HTMLButtonElement>(null);
  const settingsSlot = useRef<HTMLDivElement>(null);
  const settingsOptions = useRef<HTMLDivElement>(null);
  const [compact, setCompact] = useState(false);
  const [setupOpen, setSetupOpen] = useState(false);
  useLayoutEffect(() => {
    const row = toolbar.current;
    const slot = settingsSlot.current;
    const options = settingsOptions.current;
    if (!row || !slot || !options) return;
    const measure = () => setCompact(row.clientWidth < 500 || options.getBoundingClientRect().width > slot.clientWidth);
    measure();
    const resize = new ResizeObserver(measure);
    resize.observe(row);
    resize.observe(slot);
    resize.observe(options);
    return () => resize.disconnect();
  }, [compact, setupOpen, !!props.settings]);
  const draftKey = props.draft ?? props.appendFor;
  const [kept] = useState(() => draftOf<ComposerDraft>(draftKey));
  const [text, setText] = useState(kept?.text ?? "");
  // Parts of answers being replied to: cards above the text, each with its
  // own comment, sent before the text (compose.ts).
  const [quotes, setQuotes] = useState<(Quote & { key: string })[]>(kept?.quotes ?? []);
  // The card whose comment field takes the focus once it is on screen.
  const [focusQuote, setFocusQuote] = useState<string | null>(null);
  // While they wait, the quoted parts are marked in the answers.
  useQuoteMarks(quotes);
  // Terminal rows attached as context: cards too, sent as fenced blocks.
  const [snippets, setSnippets] = useState<(Snippet & { key: string })[]>(kept?.snippets ?? []);
  const [files, setFiles] = useState<PendingFile[]>(kept?.files ?? []);
  const [sending, setSending] = useState(false);
  const [dragging, setDragging] = useState(false);
  // The upload under way or done for each file, and the target it went to.
  const uploads = useRef(kept?.uploads ?? new Map<string, Upload>());
  const picker = useRef<HTMLInputElement>(null);
  const targetKey = JSON.stringify(props.target);
  // Start one line tall and grow with the text, up to a cap.
  const area = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (!props.appendFor) return;
    return onAppend(props.appendFor, (added) => {
      const more = added.quotes ?? [];
      const newKey = () => `q${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
      if (more.length) {
        const cards = more.map((q) => ({ ...q, key: newKey() }));
        setQuotes((q) => [...q, ...cards]);
        // A new quote waits for its comment: its field, not the box, gets the caret.
        if (added.focusQuote) {
          setFocusQuote(cards[cards.length - 1].key);
          return;
        }
      }
      const rows = added.snippets ?? [];
      if (rows.length) setSnippets((list) => [...list, ...rows.map((r) => ({ ...r, key: newKey() }))]);
      const typed = added.text;
      if (typed) setText((t) => (t && !t.endsWith("\n") ? t + "\n" : t) + typed);
      const back = added.attachments ?? [];
      if (back.length) {
        const list = back.map((a): PendingFile => ({ key: newKey(), name: a.name, size: a.size, state: "ready" }));
        list.forEach((f, i) => uploads.current.set(f.key, { target: targetKey, promise: Promise.resolve(back[i]) }));
        setFiles((cur) => [...cur, ...list]);
      }
      const el = area.current;
      if (!el) return;
      el.focus();
      // After React writes the new value, put the caret at the end.
      requestAnimationFrame(() => {
        el.selectionStart = el.selectionEnd = el.value.length;
        el.scrollTop = el.scrollHeight;
      });
    });
  }, [props.appendFor]);
  useEffect(() => {
    const el = area.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  }, [text]);

  const patch = (key: string, change: Partial<PendingFile>) =>
    setFiles((list) => list.map((f) => (f.key === key ? { ...f, ...change } : f)));

  const settle = (key: string, entry: Upload) =>
    entry.promise.then(
      // A newer upload of the same file (the host changed) owns the chip.
      () => uploads.current.get(key) === entry && patch(key, { state: "ready" }),
      (e) => uploads.current.get(key) === entry && patch(key, { state: "error", error: String(e?.message ?? e) }),
    );

  const startUpload = (f: PendingFile) => {
    const promise = props.upload!(f.file!, props.target);
    const entry = { target: targetKey, promise };
    uploads.current.set(f.key, entry);
    patch(f.key, { state: "uploading", error: undefined });
    settle(f.key, entry);
    return promise;
  };

  // A kept draft's uploads may have finished while its session was not shown.
  useEffect(() => {
    kept?.files.forEach((f) => {
      const u = uploads.current.get(f.key);
      if (f.state === "uploading" && u) settle(f.key, u);
    });
  }, []);

  useEffect(() => {
    if (!draftKey) return;
    const empty = text === "" && quotes.length === 0 && snippets.length === 0 && files.length === 0;
    keepDraft<ComposerDraft>(draftKey, empty ? null : { text, quotes, snippets, files, uploads: uploads.current });
  }, [draftKey, text, quotes, snippets, files]);

  const addFiles = (list: File[]) => {
    if (!props.upload || !list.length) return;
    const added = list.map((file): PendingFile => {
      const big = tooLarge(file.size);
      return {
        key: `f${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`,
        name: file.name,
        size: file.size,
        file,
        preview: !big && file.type.startsWith("image/") ? URL.createObjectURL(file) : undefined,
        state: big ? "error" : "uploading",
        error: big ?? undefined,
      };
    });
    setFiles((cur) => [...cur, ...added]);
    added.filter((f) => f.state !== "error").forEach(startUpload);
    area.current?.focus();
  };

  const removeFile = (key: string) => {
    uploads.current.delete(key);
    const gone = files.find((f) => f.key === key);
    if (gone?.preview) URL.revokeObjectURL(gone.preview);
    setFiles((list) => list.filter((f) => f.key !== key));
    // The clicked button is gone; Enter should still send.
    area.current?.focus();
  };

  // Files go to the host the prompt runs on; picking another host re-sends them.
  useEffect(() => {
    files.forEach((f) => {
      const u = uploads.current.get(f.key);
      if (u && u.target !== targetKey && f.file) startUpload(f);
    });
  }, [targetKey]);

  // Previews are freed when the composer goes away, unless its session keeps
  // them in a draft (then removing or sending the file frees them).
  const live = useRef(files);
  live.current = files;
  useEffect(() => () => !draftKey && live.current.forEach((f) => f.preview && URL.revokeObjectURL(f.preview)), []);

  const failed = files.some((f) => f.state === "error");
  const crowded = tooMany(files.length);
  // Quotes alone can go, like files alone: "this" with the part pointed at.
  const canSend = (text.trim() !== "" || quotes.length > 0 || snippets.length > 0 || files.length > 0) && !failed && !crowded && !props.busy && !sending;
  const submit = async () => {
    const value = withQuotes(
      quotes.map((q) => ({ text: q.text, note: q.note })),
      text.trim(),
      snippets.map(({ label, text }) => ({ label, text })),
    );
    if (!canSend) return;
    setSending(true);
    try {
      const attachments = await Promise.all(
        files.map((f) => {
          const u = uploads.current.get(f.key);
          return u && u.target === targetKey ? u.promise : startUpload(f);
        }),
      );
      if ((await props.onSubmit(value, attachments)) === false) return;
      voice.sent();
      if (!props.keepText) {
        setText("");
        setQuotes([]);
        setSnippets([]);
        files.forEach((f) => f.preview && URL.revokeObjectURL(f.preview));
        uploads.current.clear();
        setFiles([]);
        // Cleared here too: the box may have unmounted while the send waited on uploads.
        if (draftKey) keepDraft(draftKey, null);
      }
    } catch {
      // The chip that failed says why.
    } finally {
      setSending(false);
    }
  };
  // Dictation writes into the box; voice mode also sends after a pause.
  const textNow = useRef(text);
  textNow.current = text;
  const voice = useVoice({
    text: () => textNow.current,
    setText: (t) => {
      textNow.current = t;
      setText(t);
    },
    submit: () => void submit(),
    cue: props.voice?.cue,
    scope: props.voice?.scope,
    carry: props.voice?.carry,
  });
  const voiceOn = voice.mode.phase !== "off";
  // `@` offers the agents; the pick replaces what was typed after it.
  const [caret, setCaret] = useState(0);
  const [pick, setPick] = useState(0);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const mention = props.mentions?.length ? mentionAt(text, caret) : null;
  const offered = mention && dismissed !== text ? matchAgents(props.mentions!, mention.query) : [];
  useEffect(() => setPick(0), [mention?.query, offered.length]);
  const routed = props.mentions ? leadingMention(text, props.mentions) : null;
  const hint = routed && props.mentionHint ? props.mentionHint(routed.agent) : null;
  const choose = (agent: Agent) => {
    if (!mention) return;
    const next = insertMention(text, mention.start, caret, handleOf(agent));
    setText(next.text);
    setCaret(next.caret);
    requestAnimationFrame(() => {
      const el = area.current;
      if (!el) return;
      el.focus();
      el.selectionStart = el.selectionEnd = next.caret;
    });
  };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (offered.length && !e.nativeEvent.isComposing) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        setPick((p) => (p + (e.key === "ArrowDown" ? 1 : offered.length - 1)) % offered.length);
        return;
      }
      if ((e.key === "Enter" && !e.shiftKey) || e.key === "Tab") {
        e.preventDefault();
        choose(offered[Math.min(pick, offered.length - 1)]);
        return;
      }
      if (e.key === "Escape") {
        // Only the list closes, not the page or the drawer behind it.
        e.preventDefault();
        e.stopPropagation();
        setDismissed(text);
        return;
      }
    }
    if (e.key === "ArrowUp" && text === "" && props.onEditLastQueued?.()) {
      e.preventDefault();
      return;
    }
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      submit();
    }
  };
  return (
    <div
      className={`composer ${props.large ? "composer-large" : ""} ${dragging ? "composer-drop" : ""}`}
      onDragOver={(e) => {
        if (!props.upload || !dragHasFiles(e.dataTransfer)) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "copy";
        setDragging(true);
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDragging(false);
      }}
      onDrop={(e) => {
        setDragging(false);
        if (!e.dataTransfer.files.length) return;
        e.preventDefault();
        addFiles(Array.from(e.dataTransfer.files));
      }}
    >
      {voiceOn && (
        <VoiceBar
          state={voice.mode.phase === "waiting" && props.voice?.approval ? "Waiting for your approval" : VOICE_PHASE[voice.mode.phase]}
          phase={voice.mode.phase}
          canSpeak={voice.canSpeak}
          onStop={voice.stopSpeaking}
          onEnd={voice.toggleVoice}
        />
      )}
      {files.length > 0 && (
        <div className="attachments" aria-label="Attachments">
          {files.map((f) => (
            <PendingChip key={f.key} file={f} onRemove={() => removeFile(f.key)} />
          ))}
        </div>
      )}
      {crowded && (
        <div className="attachments-error error-line small" role="alert">
          {crowded}
        </div>
      )}
      {quotes.length > 0 && (
        <div className="composer-quotes" aria-label="Quoted">
          {quotes.map((q, i) => (
            <QuoteCard
              key={q.key}
              text={q.text}
              number={quotes.length > 1 ? i + 1 : undefined}
              note={q.note ?? ""}
              focus={focusQuote === q.key}
              onFocused={() => setFocusQuote(null)}
              onNote={(note) => setQuotes((list) => list.map((x) => (x.key === q.key ? { ...x, note } : x)))}
              onShow={q.range ? () => showQuoted(q.range!) : undefined}
              onDone={() => area.current?.focus()}
              onSend={() => void submit()}
              onRemove={() => {
                setQuotes((list) => list.filter((x) => x.key !== q.key));
                area.current?.focus();
              }}
            />
          ))}
        </div>
      )}
      {snippets.length > 0 && (
        <div className="composer-quotes" aria-label="Terminal output">
          {snippets.map((sn) => (
            <QuoteCard
              key={sn.key}
              text={sn.text}
              label={`${sn.label} · ${lineCount(sn.text) === 1 ? "1 line" : `${lineCount(sn.text)} lines`}`}
              removeLabel="Remove terminal output"
              onRemove={() => {
                setSnippets((list) => list.filter((x) => x.key !== sn.key));
                area.current?.focus();
              }}
            />
          ))}
        </div>
      )}
      {hint && (
        <div className="mention-route small" role="status">
          <AgentAvatar agent={routed!.agent} size="xs" />
          {hint}
        </div>
      )}
      {offered.length > 0 && (
        <div className="mention-list" id="mention-list" role="listbox" aria-label="Agents">
          {offered.map((a, i) => (
            <button className={`picker-item menu-item ${i === pick ? "active" : ""}`}
              key={a.id}
              id={`mention-${a.id}`}
              role="option"
              aria-selected={i === pick}
              // Keep the caret in the box while the pick is made.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => choose(a)}
            >
              <AgentAvatar agent={a} size="sm" />
              <span className="mention-name">{a.name}</span>
              {/* What gets typed, when it reads differently from the name. */}
              {handleOf(a) !== a.name && <span className="muted small mention-handle">@{handleOf(a)}</span>}
              {a.about && <span className="muted small mention-about">{a.about}</span>}
            </button>
          ))}
        </div>
      )}
      <textarea
        ref={area}
        aria-label="Message"
        rows={props.rows}
        maxLength={props.maxLength}
        placeholder={props.placeholder}
        value={text}
        aria-autocomplete={props.mentions ? "list" : undefined}
        aria-expanded={props.mentions ? offered.length > 0 : undefined}
        aria-controls={offered.length ? "mention-list" : undefined}
        aria-activedescendant={offered.length ? `mention-${offered[Math.min(pick, offered.length - 1)].id}` : undefined}
        onChange={(e) => {
          setText(e.target.value);
          setCaret(e.target.selectionStart ?? e.target.value.length);
          voice.typed(e.target.value);
        }}
        onSelect={(e) => setCaret(e.currentTarget.selectionStart ?? 0)}
        onKeyDown={onKeyDown}
        onPaste={(e) => {
          if (!props.upload) return;
          const pasted = pastedFiles(e.clipboardData);
          if (!pasted.length) return;
          e.preventDefault();
          addFiles(pasted);
        }}
        autoFocus
      />
      <div ref={toolbar} className={`composer-row ${props.settings ? "session-toolbar" : ""}`}>
        <div className="composer-controls">
          <button className="btn-icon attach-btn" aria-label="Attach files" disabled={!props.upload} title={props.upload ? "Attach files" : "Attachments are not supported here"} onClick={() => picker.current?.click()}>
            <Icon d={PAPERCLIP} size={16} />
          </button>
          {!voiceOn && (
            <button
              className={`btn-icon attach-btn mic-btn${voice.dictating ? " listening" : ""}`}
              aria-label={voice.unavailable ? "Dictate (not available here)" : voice.dictating ? "Stop dictation" : "Dictate"}
              aria-pressed={voice.dictating}
              aria-disabled={voice.unavailable ? true : undefined}
              title={voice.unavailable ?? (voice.dictating ? "Stop dictation (Esc)" : "Dictate")}
              onClick={voice.toggleDictation}
            >
              <Icon d={voice.dictating ? STOP : MIC} size={16} />
            </button>
          )}
          {props.voice && (
            <button
              className={`btn-icon attach-btn voice-btn${voiceOn ? " on" : ""}`}
              aria-label="Voice mode"
              aria-disabled={voice.unavailable ? true : undefined}
              aria-pressed={voiceOn}
              title={voice.unavailable ?? (voiceOn ? "Turn off voice mode (Esc)" : "Voice mode: talk, it sends after a pause and reads the answer aloud")}
              onClick={voice.toggleVoice}
            >
              <Icon d={WAVES} size={16} />
            </button>
          )}
          <input
            ref={picker}
            type="file"
            disabled={!props.upload}
            multiple
            hidden
            aria-label="Attach files"
            onChange={(e) => {
              addFiles(Array.from(e.currentTarget.files ?? []));
              // Picking the same file again must fire a change.
              e.currentTarget.value = "";
            }}
          />
          {props.settings ? (
            <div ref={settingsSlot} className={`composer-settings-slot ${compact ? "compact" : ""}`}>
              {!setupOpen && (
                <div key={compact ? "measure" : "options"} ref={settingsOptions} className="composer-settings-options" inert={compact || undefined}>
                  {props.children}
                </div>
              )}
              {compact && (
                <button
                  ref={setup}
                  className={`btn-quiet composer-setup ${props.settings.full ? "permission-full" : ""}`}
                  aria-label={`${props.setupTitle ?? "Session setup"}: ${props.settings.label}`}
                  aria-haspopup="dialog"
                  aria-expanded={setupOpen}
                  title={props.settings.label}
                  onClick={() => setSetupOpen(!setupOpen)}
                >
                  <span className="setup-model">{props.settings.model}</span>
                  {!props.settings.full && props.settings.effort && <span className="setup-effort">· {capitalize(props.settings.effort)}</span>}
                  {props.settings.full && <Icon d="M12 22s7-3 7-10V5l-7-3-7 3v7c0 7 7 10 7 10Z" size={14} />}
                  <Icon d={CHEVRON_DOWN} size={12} />
                </button>
              )}
              {compact && setupOpen && (
                <AnchoredPopover anchor={setup} label={props.setupTitle ?? "Session setup"} variant="session-setup-popover" onClose={() => setSetupOpen(false)}>
                  <div className="popover-heading"><strong>{props.setupTitle ?? "Session setup"}</strong></div>
                  <div className="setup-options">{props.children}</div>
                </AnchoredPopover>
              )}
            </div>
          ) : props.children}
        </div>
        {!props.settings && <span className="spacer" />}
        <button
          className="send"
          aria-label={props.queueing ? "Add to queue" : "Send"}
          title={props.queueing ? props.queueing.charAt(0).toUpperCase() + props.queueing.slice(1) : undefined}
          disabled={!canSend}
          onClick={submit}
        >
          {sending ? <span className="send-wait" aria-label="Uploading" /> : <Icon d={ARROW_UP} width={2.5} />}
        </button>
      </div>
      {voice.error && (
        <div className="voice-error error-line small" role="alert">
          {voice.error}
        </div>
      )}
    </div>
  );
}

const VOICE_PHASE: Record<string, string> = {
  listening: "Listening",
  waiting: "Waiting for the agent",
  speaking: "Reading aloud",
};

/** Voice mode's state over the reply box, with the ways out of it. */
function VoiceBar({ state, phase, canSpeak, onStop, onEnd }: { state: string; phase: string; canSpeak: boolean; onStop(): void; onEnd(): void }) {
  return (
    <div className={`voice-bar voice-${phase}`}>
      <span className="voice-dot" aria-hidden="true" />
      <span className="voice-state" role="status">
        Voice mode · {state}
      </span>
      {!canSpeak && <span className="muted small">Answers can't be read aloud in this browser</span>}
      <span className="spacer" />
      {phase === "speaking" && (
        <button className="btn-text" aria-label="Stop reading aloud" onClick={onStop}>
          Stop
        </button>
      )}
      <button className="btn-text" aria-label="Turn off voice mode" onClick={onEnd}>
        Turn off
      </button>
    </div>
  );
}

/** A quote waiting in the reply box: two lines, More shows the rest. A
 *  quote of an answer (not terminal output) has its own comment field, so
 *  each comment goes with its part; numbered when there are several, as the
 *  marks in the answers are. */
function QuoteCard(props: {
  text: string;
  label?: string;
  removeLabel?: string;
  number?: number;
  note?: string;
  focus?: boolean;
  onFocused?(): void;
  onNote?(note: string): void;
  /** Scroll the quoted part of the answer into view. */
  onShow?(): void;
  /** Enter in the comment: back to the reply box. */
  onDone?(): void;
  onSend?(): void;
  onRemove(): void;
}) {
  const { text, label, removeLabel = "Remove quote", number, onRemove } = props;
  const [open, setOpen] = useState(false);
  const [cut, setCut] = useState(false);
  const body = useRef<HTMLDivElement>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const el = body.current;
    if (el && !open) setCut(el.scrollHeight > el.clientHeight + 1);
  }, [text, open]);
  useEffect(() => {
    if (!props.focus) return;
    field.current?.focus();
    props.onFocused?.();
  }, [props.focus]);
  // The comment grows with its text, up to a few lines.
  useLayoutEffect(() => {
    const el = field.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 120)}px`;
  }, [props.note]);
  const commented = props.onNote !== undefined;
  return (
    <div className={`quote-card${open ? " open" : ""}${label ? " snippet" : ""}${commented ? " commented" : ""}`}>
      {label && <div className="quote-card-label">{label}</div>}
      {number !== undefined &&
        (props.onShow ? (
          <button className="quote-card-number" title="Show this part in the answer" aria-label={`Show part ${number} in the answer`} onClick={props.onShow}>
            {number}
          </button>
        ) : (
          <span className="quote-card-number" aria-hidden="true">
            {number}
          </span>
        ))}
      <div ref={body} className="quote-card-text">
        {text}
      </div>
      {(cut || open) && (
        <button className="btn-text" aria-expanded={open} onClick={() => setOpen(!open)}>
          {open ? "Less" : "More"}
        </button>
      )}
      <button className="btn-icon" aria-label={removeLabel} title={removeLabel} onClick={onRemove}>
        <Icon d={CLOSE} size={12} />
      </button>
      {commented && (
        <textarea
          ref={field}
          className="quote-card-note"
          rows={1}
          value={props.note}
          placeholder="Comment on this part (optional)"
          aria-label={number !== undefined ? `Comment on part ${number}` : "Comment on this part"}
          onChange={(e) => props.onNote!(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              props.onSend?.();
            } else if ((e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) || e.key === "Escape") {
              e.preventDefault();
              props.onDone?.();
            }
          }}
        />
      )}
    </div>
  );
}

/** The quoted parts waiting in a reply box, marked in the answers they came
 *  from (CSS Custom Highlight API, where the browser has it). One box shows
 *  at a time, so one name serves. */
function useQuoteMarks(quotes: { range?: Range }[]) {
  useEffect(() => {
    const css = (globalThis as any).CSS;
    const Highlight = (globalThis as any).Highlight;
    if (!css?.highlights || !Highlight) return;
    const ranges = quotes.map((q) => q.range).filter((r): r is Range => !!r && r.startContainer.isConnected);
    if (ranges.length) css.highlights.set("yard-quoted", new Highlight(...ranges));
    else css.highlights.delete("yard-quoted");
    return () => css.highlights.delete("yard-quoted");
  }, [quotes]);
}

/** Bring a quoted part of an answer into view. */
function showQuoted(range: Range) {
  if (!range.startContainer.isConnected) return;
  const el = range.startContainer instanceof Element ? range.startContainer : range.startContainer.parentElement;
  el?.scrollIntoView({ block: "center", behavior: "smooth" });
}

function PendingChip({ file, onRemove }: { file: PendingFile; onRemove(): void }) {
  const [broken, setBroken] = useState(false);
  const { state } = file;
  return (
    <div className={`attachment attachment-${state}`} title={file.error ?? file.name}>
      {file.preview && !broken ? (
        <img className="attachment-thumb" src={file.preview} alt="" onError={() => setBroken(true)} />
      ) : (
        <span className="attachment-icon">
          <Icon d={FILE} size={16} />
        </span>
      )}
      <span className="attachment-text">
        <span className="attachment-name">{file.name || "file"}</span>
        <span className="attachment-meta">
          {state === "error" ? file.error : state === "uploading" ? `${humanSize(file.size)} · uploading` : humanSize(file.size)}
        </span>
      </span>
      <button className="attachment-remove" aria-label={`Remove ${file.name}`} onClick={onRemove}>
        <Icon d={CLOSE} size={12} />
      </button>
    </div>
  );
}

/** An attachment of a sent prompt. Images load their bytes from the
 *  session's host; other files show their name, with the path on hover. */
function SentAttachment({ attachment: a, load }: { attachment: Attachment; load(a: Attachment): Promise<string> }) {
  const image = isInlineImage(a.mime);
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    if (!image) return;
    let live = true;
    load(a).then(
      (u) => live && setUrl(u),
      () => live && setFailed(true),
    );
    return () => {
      live = false;
    };
  }, [a.id]);
  if (image && !failed) {
    return (
      <a className="sent-image" href={url ?? undefined} target="_blank" rel="noreferrer" title={`${a.name}\n${a.path}`}>
        {url ? <img src={url} alt={a.name} /> : <span className="sent-image-wait" aria-label={`Loading ${a.name}`} />}
      </a>
    );
  }
  return (
    <div className="attachment attachment-ready" title={a.path}>
      <span className="attachment-icon">
        <Icon d={FILE} size={16} />
      </span>
      <span className="attachment-text">
        <span className="attachment-name">{a.name}</span>
        <span className="attachment-meta">{humanSize(a.size)}</span>
      </span>
    </div>
  );
}

/* ---------- new session ---------- */

/** The New session box's draft key: it has no session yet (compose.ts). */
const NEW_TASK_DRAFT = "new";

function NewSession({ state, actions, onUsageAccount }: { state: State; actions: Actions; onUsageAccount(account: UsageAccount): void }) {
  // Where the session runs. A remote host can be picked only while connected.
  const [host, setHost] = useState(loadHost);
  // A configured host stays selected while reconnecting. Never silently send
  // a remote task to another machine because its connection briefly dropped.
  const target = host === "local" || !state.connected || state.hosts.some((h) => h.id === host) ? host : "local";
  const online = hostOnline(state, target);
  const models = modelsOn(state, target);
  const home = homeOn(state, target);
  const workspace = state.workspace[target] || home;
  const [loggingIn, setLoggingIn] = useState(false);
  const [choice, setChoice] = useState<Choice>(() => loadChoice(modelsOn(state, target), target, host));
  const choicePicked = useRef(false);
  const choose = (next: Choice) => { choicePicked.current = true; setChoice(next); };
  const [speed, setSpeed] = useState(STANDARD_SPEED.id);
  const tiers = choice.engine === "codex" ? models.codex?.find((model) => model.id === choice.model)?.serviceTiers : undefined;
  const serviceTier = tiers?.length ? tiers.find((tier) => tier.id === speed)?.id ?? STANDARD_SPEED.id : undefined;
  // The account per engine on this host, remembered like the model. One that
  // was removed meanwhile falls back to Default.
  const accounts = accountsOn(state, target);
  const [picked, setPicked] = useState<Record<string, string>>({});
  const pickKey = (engine: Engine) => `yard.account.${target}.${engine}`;
  const pickOf = (engine: Engine): string => {
    const wanted = picked[pickKey(engine)] ?? localStorage.getItem(pickKey(engine)) ?? DEFAULT_ACCOUNT;
    return accountChoices(accounts, engine).some((a) => a.id === wanted) ? wanted : DEFAULT_ACCOUNT;
  };
  const picks = Object.fromEntries(ENGINES.map((e) => [e, pickOf(e)])) as Record<Engine, string>;
  const selectedAccount = picks[choice.engine];
  useEffect(() => onUsageAccount({ host: target, engine: choice.engine, account: selectedAccount }), [onUsageAccount, target, choice.engine, selectedAccount]);
  const pickAccount = (engine: Engine, id: string) => {
    localStorage.setItem(pickKey(engine), id);
    setPicked({ ...picked, [pickKey(engine)]: id });
    // Picking an account under the other engine means running on it.
    const next = engine === choice.engine ? null : choiceOnEngine(models, engine, choice.effort);
    if (next) choose(next);
  };
  const [permission, setPermission] = useState<Permission>(() => {
    const saved = localStorage.getItem("yard.permission");
    return PERMISSIONS.some((p) => p.id === saved) ? (saved as Permission) : "ask";
  });
  const [cwd, setCwd] = useState(() => loadFolder(target));
  const previousTarget = useRef(target);
  // What the worktree is cut from; empty means the folder's current branch.
  const [base, setBase] = useState("");
  // From send until yardd answers. On success this page is replaced by the
  // session; on failure the task stays in the box so nothing is retyped.
  const [pending, setPending] = useState(false);
  useEffect(() => setPending(false), [state.errorCount, state.connected]);
  // The box keeps a half-typed task while another page is open (keepText
  // leaves it in place after Send). Leaving while the create is in flight
  // means the session took it: the reply opened it, so drop the draft.
  const sent = useRef(false);
  sent.current = pending;
  useEffect(
    () => () => {
      if (!sent.current) return;
      draftOf<ComposerDraft>(NEW_TASK_DRAFT)?.files.forEach((f) => f.preview && URL.revokeObjectURL(f.preview));
      keepDraft(NEW_TASK_DRAFT, null);
    },
    [],
  );
  // Models and the home folder arrive after the first render, and differ per host.
  useEffect(() => {
    const changed = previousTarget.current !== target;
    if (changed) choicePicked.current = false;
    setChoice((current) => {
      const next = changed || (!choicePicked.current && loadDefaults("sessions")?.host === host) ? loadChoice(models, target, host) : validChoice(models, current);
      return next.engine === current.engine && next.model === current.model && next.effort === current.effort ? current : next;
    });
    if (previousTarget.current !== target) setCwd(loadFolder(target) || workspace);
    else setCwd((current) => current || loadFolder(target) || workspace);
    previousTarget.current = target;
  }, [models, target, workspace]);

  // Recent picks first, then the folders of this host's sessions. A worktree
  // session counts as the repository it was cut from.
  const recent = useMemo(() => {
    const fromSessions = Object.values(state.sessions)
      // Agents' conversations run in the agents' own folders, not projects.
      .filter((s) => hostOf(s) === target && !s.agent && !isAgentHome(s.cwd))
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .map((s) => s.repo || s.cwd);
    return [...new Set([...readRecent(target), ...fromSessions])].slice(0, 12);
  }, [state.sessions, target]);

  // Every session in a repository gets a worktree, so the branches tell
  // whether this folder is one and what it can start from.
  const branchList = state.branches[branchKey(target, cwd)];
  useEffect(() => {
    if (!cwd || !online) return;
    actions.loadBranches(cwd, target);
  }, [cwd, target, online]);
  useEffect(() => setBase(""), [cwd, target]);
  const isRepo = !!branchList?.current;
  // A remembered folder can be deleted since; Send would only be refused.
  const gone = !!branchList?.missing;

  const start = async (text: string, attachments: Attachment[]) => {
    if (/^\/login\s*$/.test(text.trim()) && !attachments.length) {
      setLoggingIn(true);
      return;
    }
    // "@Reviewer look at #12" goes to the agent's chat, not a new session.
    const routed = leadingMention(text, state.agents);
    if (routed) {
      if (!routed.rest && !attachments.length) return false;
      setPending(true);
      const r = await actions.askAgent(routed.agent.id, routed.rest, attachments.map((a) => a.id));
      setPending(false);
      if (r.error) {
        setAgentError(r.error);
        return false;
      }
      keepDraft(NEW_TASK_DRAFT, null);
      return;
    }
    setAgentError(null);
    rememberSession({ ...choice, host: target, cwd });
    localStorage.setItem("yard.permission", permission);
    setPending(true);
    actions.create({
      ...choice,
      serviceTier,
      permission,
      account: picks[choice.engine] === DEFAULT_ACCOUNT ? undefined : picks[choice.engine],
      cwd,
      text,
      worktree: true,
      base: base || undefined,
      host: target === "local" ? undefined : target,
      attachments: attachments.length ? attachments.map((a) => a.id) : undefined,
    });
  };
  const uploadTarget = useMemo<UploadTarget>(() => (target === "local" ? {} : { host: target }), [target]);
  const [agentError, setAgentError] = useState<string | null>(null);
  const toAgent = (agent: Agent) => `Goes to ${agent.name}`;

  return (
    <div className="new">
      <div className="new-inner">
        <h1>What should we work on?</h1>
        <Composer
          placeholder="Describe the task"
          rows={1}
          large
          keepText
          draft={NEW_TASK_DRAFT}
          target={uploadTarget}
          upload={actions.upload}
          onSubmit={start}
          voice={{ scope: NEW_TASK_DRAFT, cue: null, approval: false, carry: true }}
          mentions={state.agents}
          mentionHint={toAgent}
          settings={setupSettings(modelName(models, choice.engine, choice.model), accountName(accounts, choice.engine, selectedAccount), shownEffort(models, choice.engine, choice.model, choice.effort), permission)}
          busy={pending || !online || !choice.model || !cwd || gone}
        >
          <ModelPicker models={models} value={choice} engines={pickerEngines(models, choice.engine)} onChange={choose} speed={{ value: speed, onChange: setSpeed }} />
          <AccountPicker state={state} host={target} engine={choice.engine} account={picks[choice.engine]} onPick={pickAccount} onManage={() => actions.go({ page: "hosts", host: target, tab: "engines" })} />
          <EffortPicker models={models} value={choice} onChange={(effort) => choose({ ...choice, effort })} />
          <PermissionPicker engine={choice.engine} value={permission} onChange={setPermission} />
        </Composer>
        <div className="new-chips">
          <HostPicker state={state} value={target} onChange={setHost} onAddHost={() => actions.go({ page: "hosts" })} />
          <FolderPicker
            host={target}
            home={home}
            workspace={workspace}
            value={cwd}
            recent={recent}
            dirs={state.dirs}
            listDirs={actions.listDirs}
            onChange={setCwd}
          />
          {isRepo && (
            <BranchPicker current={branchList.current!} branches={branchList.branches ?? []} value={base} onChange={setBase} />
          )}
          <span className="spacer" />
          {/* Send is disabled until the host is back; say why rather than leave it grey. */}
          {target !== "local" && !online ? (
            <span className="muted small">{hostName(state, target)} is offline</span>
          ) : cwd && branchList && (gone ? (
            <span className="bad small">This folder no longer exists</span>
          ) : (
            <span className="muted small">{isRepo ? "New worktree" : "Works in the folder itself"}</span>
          ))}
        </div>
        {agentError && (
          <div className="small error-line" role="alert">
            {agentError}
          </div>
        )}
        <EngineNotice state={state} host={target} engine={choice.engine} account={picks[choice.engine]} onLogin={() => setLoggingIn(true)} />
        {loggingIn && (
          <LoginDialog state={state} actions={actions} host={target} engine={choice.engine} account={picks[choice.engine]} onClose={() => setLoggingIn(false)} />
        )}
      </div>
    </div>
  );
}

const FOLDER = "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z";
const BRANCH = "M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM18 9a9 9 0 0 1-9 9";
const LAPTOP = "M4 5h16v11H4ZM2 20h20";
const CHECK = "M5 12l5 5 9-11";

/** Close an open menu on Escape, wherever focus is. */
export function useEscape(open: boolean, close: () => void) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: globalThis.KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);
}

/** A button under the prompt box that opens a menu below it. */
function ChipMenu(props: {
  icon: ReactNode;
  label: ReactNode;
  menuLabel: string;
  title?: string;
  open: boolean;
  setOpen(open: boolean): void;
  wide?: boolean;
  children: ReactNode;
}) {
  const { open, setOpen } = props;
  const anchor = useRef<HTMLButtonElement>(null);
  return (
    <div className={`picker${props.wide ? " picker-wide" : ""}`}>
      <button ref={anchor} className="chip" aria-haspopup="dialog" aria-expanded={open} title={props.title} onClick={() => setOpen(!open)}>
        {props.icon}
        <span className="chip-label">{props.label}</span>
        <Icon d={CHEVRON_DOWN} size={12} />
      </button>
      {open && (
        <AnchoredPopover anchor={anchor} label={props.menuLabel} variant={`chip-popover${props.wide ? " chip-popover-wide" : ""}`} onClose={() => setOpen(false)}>
          <div className="chip-menu-content">
            {props.children}
          </div>
        </AnchoredPopover>
      )}
    </div>
  );
}

export function HostPicker(props: { state: State; value: string; onChange(host: string): void; onAddHost(): void; allowOffline?: boolean }) {
  const { state, value } = props;
  const [open, setOpen] = useState(false);
  const pick = (host: string) => {
    props.onChange(host);
    setOpen(false);
  };
  const remote = state.hosts.find((h) => h.id === value);
  return (
    <ChipMenu
      menuLabel="Hosts"
      icon={remote ? <Icon d={CLOUD} /> : <Icon d={LAPTOP} />}
      label={remote ? remote.name : hostName(state, "local")}
      title="Where the session runs"
      open={open}
      setOpen={setOpen}
    >
      <button className={`picker-item menu-item ${value === "local" ? "active" : ""}`} onClick={() => pick("local")}>
        <Icon d={LAPTOP} />
        <span className="menu-label">{hostName(state, "local")}</span>
      </button>
      {state.hosts.map((h) => {
        const online = hostOnline(state, h.id);
        return (
          <button key={h.id} className={`picker-item menu-item ${value === h.id ? "active" : ""}`} disabled={!online && !props.allowOffline} onClick={() => pick(h.id)}>
            <Icon d={CLOUD} />
            <span className="menu-label">{h.name}</span>
            {!online && <span className="muted small menu-aside">{h.status === "error" ? "unreachable" : h.status}</span>}
          </button>
        );
      })}
      <div className="menu-divider" />
      <button className="picker-item menu-item" onClick={props.onAddHost}>
        <Icon d={PLUS} />
        <span>Add a host…</span>
      </button>
    </ChipMenu>
  );
}

/** The desktop app's window exposes Tauri's IPC; a plain browser does not. */
const tauriInvoke = (): ((cmd: string, args: object) => Promise<unknown>) | undefined =>
  (window as any).__TAURI__?.core?.invoke;

const parentOf = (path: string) => (path.lastIndexOf("/") <= 0 ? "/" : path.slice(0, path.lastIndexOf("/")));
const joinPath = (dir: string, name: string) => (dir.endsWith("/") ? dir + name : `${dir}/${name}`);
const baseName = (path: string) => path.slice(path.lastIndexOf("/") + 1) || path;

/** Recent folders, a search that browses the host's folders as a path is
 *  typed, and the Finder dialog when the session runs on this Mac. */
export function FolderPicker(props: {
  host: string;
  home: string;
  /** The folder whose subfolders are offered first: the host's projects. */
  workspace: string;
  value: string;
  recent: string[];
  dirs: State["dirs"];
  listDirs(path: string, host: string): void;
  onChange(path: string): void;
  /** Shown when no folder is chosen. */
  placeholder?: string;
}) {
  const { host, home, workspace, value, recent } = props;
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (open) {
      setQuery("");
      setCursor(0);
    }
  }, [open]);

  // A query starting with / or ~ is a path: list the folder it ends in and
  // match the last segment against its subfolders.
  const browsing = query.startsWith("/") || query.startsWith("~");
  const expanded = query.replace(/^~(?=\/|$)/, home);
  const dir = browsing ? (expanded.endsWith("/") ? expanded.replace(/(.)\/+$/, "$1") : parentOf(expanded)) : "";
  const stem = browsing && !expanded.endsWith("/") ? baseName(expanded) : "";
  const listing = browsing ? props.dirs[branchKey(host, dir)] : undefined;
  useEffect(() => {
    if (!open || !browsing || !dir) return;
    const timer = setTimeout(() => props.listDirs(dir, host), 120);
    return () => clearTimeout(timer);
  }, [open, browsing, dir, host]);

  // Without a path typed, the workspace's folders are listed under the recent ones.
  const projects = props.dirs[branchKey(host, workspace)];
  useEffect(() => {
    if (open && workspace) props.listDirs(workspace, host);
  }, [open, workspace, host]);

  type Row = { path: string; name: string; detail: string; git?: boolean; browse?: boolean; section?: string };
  const rows: Row[] = useMemo(() => {
    if (browsing) {
      const lower = stem.toLowerCase();
      const children = (listing?.dirs ?? [])
        .filter((d) => d.name.toLowerCase().startsWith(lower) && (lower.startsWith(".") || !d.name.startsWith(".")))
        .slice(0, 200)
        .map((d) => ({ path: joinPath(dir, d.name), name: d.name, detail: "", git: d.git, browse: true }));
      // The folder typed so far is itself a choice, unless yardd says it is not a folder there.
      const self = !stem && dir && !listing?.missing && !listing?.file && !listing?.error ? [{ path: dir, name: `Use ${tilde(dir, home)}`, detail: "" }] : [];
      return [...self, ...children];
    }
    const lower = query.toLowerCase();
    // A recent folder is browsable like any other, so Tab on the highlighted
    // one opens it instead of moving focus out of the search. Its git flag
    // comes from the workspace listing it would otherwise appear in.
    const gitIn = new Map((projects?.dirs ?? []).map((d) => [joinPath(workspace, d.name), d.git]));
    const recents = recent
      .filter((p) => p.toLowerCase().includes(lower))
      .slice(0, lower ? 12 : 5)
      .map((p) => ({ path: p, name: baseName(p), detail: tilde(p, home), git: gitIn.get(p), browse: true, section: "Recent" }));
    const shown = new Set(recents.map((r) => r.path));
    const inWorkspace = (projects?.dirs ?? [])
      .filter((d) => !d.name.startsWith(".") && d.name.toLowerCase().includes(lower))
      .map((d) => ({ path: joinPath(workspace, d.name), name: d.name, detail: "", git: d.git, browse: true, section: `In ${tilde(workspace, home)}` }))
      .filter((r) => !shown.has(r.path))
      .slice(0, 200);
    return [...recents, ...inWorkspace];
  }, [browsing, stem, listing, dir, query, recent, home, projects, workspace]);
  useEffect(() => setCursor(0), [query]);
  useEffect(() => {
    list.current?.querySelector<HTMLElement>(`[data-row="${cursor}"]`)?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  const pick = (path: string) => {
    props.onChange(path);
    setOpen(false);
  };
  const browseInto = (path: string) => setQuery(tilde(path, home) + "/");
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    const row = rows[cursor];
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursor((c) => Math.min(c + 1, rows.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursor((c) => Math.max(c - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (row) pick(row.path);
    } else if (e.key === "Tab" && row?.browse) {
      e.preventDefault();
      browseInto(row.path);
    }
  };
  const invoke = host === "local" ? tauriInvoke() : undefined;
  const chooseInFinder = async () => {
    setOpen(false);
    try {
      const picked = await invoke!("plugin:dialog|open", { options: { directory: true, multiple: false, defaultPath: value || home } });
      if (typeof picked === "string") props.onChange(picked);
    } catch (err) {
      console.error("folder dialog failed", err);
    }
  };

  return (
    <ChipMenu icon={<Icon d={FOLDER} />} label={value ? baseName(value) : (props.placeholder ?? "Choose a folder")} menuLabel="Folders" title={value} open={open} setOpen={setOpen} wide>
      <input
        className="menu-search"
        autoFocus
        placeholder="Search folders, or type a path"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={onKey}
        spellCheck={false}
        aria-label="Folder"
      />
      <div className="menu-list" ref={list} role="listbox">
        {rows.map((row, i) => [
          row.section && row.section !== rows[i - 1]?.section && (
            <div key={`head-${row.section}`} className="picker-head">
              {row.section}
            </div>
          ),
          <div
            key={row.path + i}
            data-row={i}
            role="option"
            aria-selected={i === cursor}
            className={`picker-item menu-item folder-row ${i === cursor ? "cursor" : ""} ${row.path === value ? "active" : ""}`}
            onMouseEnter={() => setCursor(i)}
            onClick={() => pick(row.path)}
          >
            <Icon d={row.git ? BRANCH : FOLDER} />
            <span className="folder-name">{row.name}</span>
            {row.detail && <span className="muted small folder-detail">{row.detail}</span>}
            {row.path === value && <Icon d={CHECK} />}
            {row.browse && (
              <button
                className="btn-icon folder-open"
                aria-label={`Open ${row.name}`}
                title="Show the folders inside (Tab)"
                onClick={(e) => {
                  e.stopPropagation();
                  browseInto(row.path);
                }}
              >
                <Icon d={CHEVRON_RIGHT} size={12} />
              </button>
            )}
          </div>,
        ])}
        {browsing && listing?.missing && <div className="picker-empty">{tilde(listing.missing, home)} doesn’t exist.</div>}
        {browsing && listing?.file && <div className="picker-empty">{tilde(listing.file, home)} is a file, not a folder.</div>}
        {browsing && listing?.error && <div className="picker-empty">{listing.error}</div>}
        {/* Only a typed last segment can leave this list empty ("Use <dir>" is offered otherwise); say why Enter does nothing. */}
        {browsing && listing && !listing.error && !listing.missing && !listing.file && rows.length === 0 && (
          <div className="picker-empty">
            No folder in {tilde(dir, home)} starts with “{stem}”.
          </div>
        )}
        {browsing && !listing && <div className="picker-empty">Loading…</div>}
        {!browsing && rows.length === 0 && (
          <div className="picker-empty">{!projects && !query ? "Loading…" : query ? "No folder matches. Start with / or ~ to type a path." : "No folders here yet. Start with / or ~ to type a path."}</div>
        )}
      </div>
      {(!browsing || invoke) && <div className="menu-divider" />}
      {!browsing && (
        <button className="picker-item menu-item" onClick={() => browseInto(workspace || home || "/")}>
          <Icon d={FOLDER} />
          <span>Browse folders…</span>
        </button>
      )}
      {invoke && (
        <button className="picker-item menu-item" onClick={chooseInFinder}>
          <Icon d={FOLDER} />
          <span>Choose in Finder…</span>
        </button>
      )}
    </ChipMenu>
  );
}

/** What the new worktree is cut from. */
export function BranchPicker(props: { current: string; branches: string[]; value: string; onChange(base: string): void }) {
  const { current, value } = props;
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  useEffect(() => {
    if (open) setQuery("");
  }, [open]);
  const options = useMemo(() => {
    const all = [current, ...props.branches.filter((b) => b !== current)];
    const lower = query.toLowerCase();
    return all.filter((b) => b.toLowerCase().includes(lower)).slice(0, 200);
  }, [current, props.branches, query]);
  const pick = (b: string) => {
    props.onChange(b === current ? "" : b);
    setOpen(false);
  };
  const shown = value || current;
  return (
    <ChipMenu icon={<Icon d={BRANCH} />} label={shown} menuLabel="Branches" title="Branch the worktree starts from" open={open} setOpen={setOpen}>
      <input
        className="menu-search"
        autoFocus
        placeholder="Search branches"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && options[0] && pick(options[0])}
        spellCheck={false}
        aria-label="Base branch"
      />
      <div className="menu-list">
        {options.map((b) => (
          <button key={b} className={`picker-item menu-item ${b === shown ? "active" : ""}`} onClick={() => pick(b)}>
            <span className="folder-name">{b}</span>
            {b === current && <span className="muted small menu-aside">current</span>}
          </button>
        ))}
        {options.length === 0 && <div className="picker-empty">No branch matches.</div>}
      </div>
    </ChipMenu>
  );
}

/* ---------- session ---------- */

function SessionView({ meta, state, actions, before }: { meta: SessionMeta; state: State; actions: Actions; before?: React.ReactNode }) {
  const [workspace, setWorkspace] = useState(false);
  const [workspaceCollapsed, setWorkspaceCollapsed] = usePanelCollapsed("yard.workspaceCollapsed");
  const wide = !useMedia(WORKSPACE_NARROW);
  const workspaceShown = wide ? !workspaceCollapsed : workspace;
  const workspaceLabel = workspaceCollapsed ? "Expand workspace" : "Collapse workspace";
  const workspaceOpenButton = useRef<HTMLButtonElement>(null);
  const workspaceCloseButton = useRef<HTMLButtonElement>(null);
  const openWorkspace = () => {
    setWorkspace(true);
    requestAnimationFrame(() => workspaceCloseButton.current?.focus({ preventScroll: true }));
  };
  const closeWorkspace = () => {
    setWorkspace(false);
    requestAnimationFrame(() => workspaceOpenButton.current?.focus({ preventScroll: true }));
  };
  useEffect(() => setWorkspace(false), [wide]);
  const thread = state.threads[meta.id];
  const items = thread?.items ?? [];
  const changes = state.changes[meta.id];
  const busy = meta.status === "running" || meta.status === "needs_approval";
  const host = hostOf(meta);
  const online = hostOnline(state, host);
  const models = modelsOn(state, host);

  // Stick to the bottom while new items arrive, unless the user scrolled up.
  const scroller = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const lastTop = useRef(0);
  useEffect(() => {
    const el = scroller.current;
    if (el && pinned.current) el.scrollTop = el.scrollHeight;
  }, [items]);
  // Not only new items move the bottom: the Working line appears with the
  // status (sent just after the event), thumbnails load, the reply box grows
  // with queued messages and cards. While pinned, any change in the
  // thread's height or the visible area keeps the bottom in view.
  useEffect(() => {
    const el = scroller.current;
    const column = el?.firstElementChild;
    if (!el || !column) return;
    const keep = () => {
      if (pinned.current) el.scrollTop = el.scrollHeight;
    };
    const watch = new ResizeObserver(keep);
    watch.observe(column);
    watch.observe(el);
    return () => watch.disconnect();
  }, []);

  // The working tree has probably changed whenever a turn ends.
  const last = useRef(meta.status);
  useEffect(() => {
    if (last.current !== "idle" && meta.status === "idle") actions.loadChanges(meta.id);
    last.current = meta.status;
  }, [meta.status]);
  // What voice mode says when the session reaches a new moment (voice.ts).
  // What is already there when the session opens is not news.
  const said = useMemo(() => voiceCue(items), [items]);
  const saidKey = useRef(said?.key);
  const [cue, setCue] = useState<{ n: number; cue: Cue } | null>(null);
  useEffect(() => {
    if (said?.key === saidKey.current) return;
    saidKey.current = said?.key;
    if (said) setCue((c) => ({ n: (c?.n ?? 0) + 1, cue: said.cue }));
  }, [said?.key]);
  const approval = meta.status === "needs_approval";
  const voice = useMemo(() => ({ scope: meta.id, cue, approval }), [meta.id, cue, approval]);

  const running = meta.status === "running" || meta.status === "needs_approval";
  const rows = useMemo(() => layout(items, running), [items, running]);
  const handoffs = useMemo(() => handoffLinks(items, meta, state.sessions), [items, meta, state.sessions]);
  const lastItem = items[items.length - 1];
  const working = meta.status === "running" && !(lastItem?.kind === "assistant" && lastItem.streaming);
  // The open turn's start, from yardd's clock.
  const turnStart = useMemo(() => {
    for (let i = items.length - 1; i >= 0; i--) {
      const it = items[i];
      if (it.kind === "turn") return it.end == null ? (it.start ?? null) : null;
    }
    return null;
  }, [items]);
  // Files go to the session's own host, through the hub for a remote one.
  const uploadTarget = useMemo<UploadTarget>(() => ({ sessionId: meta.id }), [meta.id]);
  const [loggingIn, setLoggingIn] = useState(false);
  const accounts = accountsOn(state, host);
  const sessionAccount = accountLabel(accounts, meta);
  // The engines' own /login cannot run inside a session; the app's does it.
  const place = state.slack?.places?.find((p) => p.sessionId === meta.id);
  const agent = agentOf(state.agents, meta) ?? state.agents.find((a) => a.id === place?.agentId);
  // A conversation an agent left behind when it moved: read, not answered.
  const earlier = !!agent && (isEarlierConversation(agent, meta.id) || (!!place && host !== agentHost(agent)));
  // The agents live on the hub: a remote session's box offers them only in
  // an agent's own conversation.
  const mentionable = host === "local" || !!agent;
  // In an agent's conversation the box's pickers are the agent's: a pick
  // saves it, and the conversation follows (agents.rs `follow`).
  const agentEdit = useAgentSave(actions, earlier ? undefined : agent?.id);
  const agentMove = useAgentMove(state, earlier ? undefined : agent, agentEdit.save);
  // An agent in its own folder has no project: no terminal, subagents or
  // processes to show.
  const noPane = !!agent && !agent.cwd;
  // A file link opens the file in the workspace; where there is none (an
  // agent with no folder), it downloads.
  const [openedFile, setOpenedFile] = useState<{ path: string; nonce: number } | null>(null);
  useEffect(() => setOpenedFile(null), [meta.id]);
  const openFile = async (path: string): Promise<string | null> => {
    if (noPane) {
      const read = await actions.readFile(meta.id, path);
      if ("error" in read) return read.error;
      saveFile(read.name, read.data);
      return null;
    }
    setOpenedFile((f) => ({ path, nonce: (f?.nonce ?? 0) + 1 }));
    if (wide) setWorkspaceCollapsed(false);
    else openWorkspace();
    return null;
  };
  const submit = async (text: string, attachments: Attachment[]) => {
    if (/^\/login\s*$/.test(text.trim()) && !attachments.length) {
      setLoggingIn(true);
      return;
    }
    // A reply that starts with another agent's @Name goes to that agent's chat.
    const routed = mentionable ? leadingMention(text, state.agents) : null;
    if (routed && routed.agent.id !== agent?.id) {
      if (!routed.rest && !attachments.length) return false;
      const r = await actions.askAgent(routed.agent.id, routed.rest, attachments.map((a) => a.id));
      return r.error ? false : undefined;
    }
    // What you just sent is what you want to see: back to the bottom.
    pinned.current = true;
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
    actions.send(meta.id, text, attachments.map((a) => a.id));
  };
  const toAgent = (other: Agent) => (other.id === agent?.id ? null : `Goes to ${other.name}, not this ${agent ? "conversation" : "session"}`);
  // Whose answers these are: the agent's face and name over each turn's answer.
  const answerKeys = useMemo(() => {
    const keys = new Set<string>();
    if (!agent) return keys;
    let waiting = false;
    for (const it of items) {
      if (it.kind === "user" && !it.steered) waiting = true;
      else if (it.kind === "assistant" && waiting) {
        keys.add(it.key);
        waiting = false;
      }
    }
    return keys;
  }, [items, agent?.id]);
  // The answer being written now: its face looks busy.
  const lastAnswerKey = useMemo(() => [...answerKeys].pop(), [answerKeys]);
  const loadAttachment = (a: Attachment) => actions.attachmentUrl(meta.id, a);
  // Start anyway is out: hold the button until the session leaves the queue or
  // yardd refuses, or a double click sends a second startNow that yardd
  // answers with "this session is not queued".
  const [starting, setStarting] = useState(false);
  useEffect(() => setStarting(false), [meta.status, state.errorCount, state.connected]);
  const renderItem = (item: Item) => (
    <Fragment key={item.key}>
      {agent && answerKeys.has(item.key) && (
        <div className="agent-says">
          <AgentAvatar agent={agent} size="sm" busy={busy && item.key === lastAnswerKey} />
          <strong>{agent.name}</strong>
        </div>
      )}
    <ItemView
      item={item}
      link={handoffs[item.key]}
      onOpen={(id) => actions.go({ page: "session", id })}
      queued={meta.status === "queued"}
      starting={starting}
      loadAttachment={loadAttachment}
      onStart={() => {
        if (starting) return;
        setStarting(true);
        actions.startNow(meta.id);
      }}
      onDecide={(id, d, answers) => actions.approve(meta.id, id, d, answers)}
      asker={ENGINE_NAME[meta.engine]}
      agents={state.agents}
    />
    </Fragment>
  );

  return (
    <FileLinks.Provider value={openFile}>
    <div className={`session ${!wide && workspace ? "workspace-open" : ""}`}>
      <main className="thread">
        {agent ? (
          <AgentBar state={state} agent={agent} actions={actions} host={host} busy={busy && !earlier}>
            {busy && online && !meta.slackAccess && (
              <button className="btn-text" onClick={() => actions.interrupt(meta.id)}>
                Stop
              </button>
            )}
            {!noPane &&
              (wide ? (
                <button className="btn-icon workspace-toggle-desktop" aria-label={workspaceLabel} title={workspaceLabel} aria-controls="session-workspace" aria-expanded={workspaceShown} onClick={() => setWorkspaceCollapsed(!workspaceCollapsed)}>
                  <Icon d={RIGHT_PANEL} size={18} />
                </button>
              ) : (
                <button className="btn-text workspace-toggle" ref={workspaceOpenButton} aria-controls="session-workspace" aria-expanded={workspaceShown} onClick={openWorkspace}>Workspace</button>
              ))}
          </AgentBar>
        ) : (
        <header className="bar">
          <MenuButton />
          <h1>{meta.title}</h1>
          <code className="muted">{meta.branch || changes?.branch || tilde(meta.cwd, homeOn(state, host))}</code>
          <SessionPullRequest sessionId={meta.id} online={online} busy={busy} pr={state.pr[meta.id]} load={actions.loadPr} />
          {sessionAccount && (
            <span className="muted small session-account" title="The account this session runs under. It can't be changed mid-session.">
              {sessionAccount}
            </span>
          )}
          {host !== "local" && (
            <span className="muted small bar-host">
              <Cloud size={13} />
              {hostName(state, host)}
              {!online && " · offline"}
            </span>
          )}
          <span className="spacer" />
          <MobileSetupButton mobile={state.mobile} hostMobile={state.hostMobile} open={() => actions.go({ page: "hosts" })} />
          {wide ? (
            <button className="btn-icon workspace-toggle-desktop" aria-label={workspaceLabel} title={workspaceLabel} aria-controls="session-workspace" aria-expanded={workspaceShown} onClick={() => setWorkspaceCollapsed(!workspaceCollapsed)}>
              <Icon d={RIGHT_PANEL} size={18} />
            </button>
          ) : (
            <button className="btn-text workspace-toggle" ref={workspaceOpenButton} aria-controls="session-workspace" aria-expanded={workspaceShown} onClick={openWorkspace}>Workspace</button>
          )}
          {busy && online && (
            <button className="btn-text" onClick={() => actions.interrupt(meta.id)}>
              Stop
            </button>
          )}
          {online && (
            <DeleteButton
              key={meta.id}
              worktree={!!(meta.branch && meta.repo)}
              onConfirm={(removeWorktree) => actions.remove(meta.id, removeWorktree)}
            />
          )}
        </header>
        )}
        <div
          className="scroll"
          ref={scroller}
          onScroll={(e) => {
            const el = e.currentTarget;
            pinned.current = stillPinned(pinned.current, lastTop.current, el.scrollTop, el.scrollHeight - el.scrollTop - el.clientHeight);
            lastTop.current = el.scrollTop;
          }}
        >
          <div className="column">
            {place && agent && (
            <div className="note agent-earlier">
              {place.name}
              <button className="btn-text note-link" onClick={() => actions.go({ page: "agent", id: agent.id })}>
                Back to {agent.name}
              </button>
            </div>
          )}
          {before}
            {rows.map((row) => (
              <RowView key={row.key} row={row} render={renderItem} widget={(t) => t.app && <WidgetFrame sessionId={meta.id} app={t.app} actions={actions} />} />
            ))}
            {working && <Working since={turnStart} />}
            {/* The turn is over but subagents it started still work (yardd's
                `background`): said here, not only on the sidebar row. */}
            {!busy && !!meta.background?.length && (
              <Working label={meta.background.length === 1 ? "1 subagent still working" : `${meta.background.length} subagents still working`} since={null} />
            )}
          </div>
        </div>
        {online && !earlier && <QuoteReply sessionId={meta.id} />}
        {earlier ? (
          <div className="dock">
            <div className="muted small offline-note agent-earlier-dock">
              {agent!.name} moved to {hostName(state, agent!.host || "local")}; this conversation stays here to read.
              <button className="btn-text note-link" onClick={() => actions.go({ page: "agent", id: agent!.id })}>
                Open its conversation
              </button>
            </div>
          </div>
        ) : (
        <div className="dock">
          {!online && (
            <div className="muted small offline-note">
              {agent ? `${agent.name} runs on ${hostName(state, host)}, which is offline. Messages can't be sent until it is back.` : `${hostName(state, host)} is offline. This session comes back when it reconnects.`}
            </div>
          )}
          {online && <EngineNotice state={state} host={host} engine={meta.engine} account={accountOf(meta)} onLogin={() => setLoggingIn(true)} />}
          {loggingIn && (
            <LoginDialog state={state} actions={actions} host={host} engine={meta.engine} account={accountOf(meta)} onClose={() => setLoggingIn(false)} />
          )}
          {online && (
            <PendingList
              pending={meta.pending ?? []}
              busy={busy}
              hint={queuedHint(meta.engine)}
              onSteer={meta.status === "running" && canSteer(meta.engine) ? (id) => actions.steerQueued(meta.id, id) : undefined}
              onEdit={(id) => actions.unqueue(meta.id, id)}
              onRemove={(id) => actions.unqueue(meta.id, id, true)}
            />
          )}
          {agent && !earlier && <AgentRequests state={state} actions={actions} agent={agent} />}
          {agentMove.confirm}
          {agentEdit.error && (
            <div className="small error-line agent-edit-error" role="alert">
              {agentEdit.error}
            </div>
          )}
          <Composer
            // yardd refuses a message to a queued session, so Send stays disabled:
            // say why instead of a "Reply" that silently does nothing.
            placeholder={!online ? "Offline" : meta.status === "queued" ? "Queued: Start anyway to reply" : busy ? "Queue a message for the agent" : agent ? `Reply to ${agent.name}` : "Reply"}
            mentions={mentionable ? state.agents : undefined}
            mentionHint={toAgent}
            rows={1}
            busy={meta.status === "queued" || !online}
            queueing={busy ? queuedHint(meta.engine) : undefined}
            onEditLastQueued={() => {
              const last = meta.pending?.[meta.pending.length - 1];
              if (!last) return false;
              actions.unqueue(meta.id, last.id);
              return true;
            }}
            appendFor={meta.id}
            target={uploadTarget}
            upload={actions.upload}
            onSubmit={submit}
            voice={voice}
            settings={agent ? agentSettings(state, agent) : setupSettings(modelName(models, meta.engine, meta.model), accountName(accounts, meta.engine, accountOf(meta)), shownEffort(models, meta.engine, meta.model, meta.effort), meta.permission)}
            setupTitle={agent ? "Agent setup" : undefined}
          >
            {agent ? (
              <AgentRunPickers
                state={state}
                actions={actions}
                agent={agent}
                save={agentEdit.save}
                onHost={agentMove.pick}
                speed={{ value: meta.serviceTier ?? STANDARD_SPEED.id, onChange: (tier) => actions.setServiceTier(meta.id, tier), nextTurn: busy }}
              />
            ) : (
            <>
            <ModelPicker
              models={models}
              value={{ engine: meta.engine, model: meta.model, effort: meta.effort }}
              engines={pickerEngines(models, meta.engine)}
              onChange={(c) => actions.setModel(meta.id, c.model, c.effort)}
              speed={{ value: meta.serviceTier ?? STANDARD_SPEED.id, onChange: (tier) => actions.setServiceTier(meta.id, tier), nextTurn: busy }}
              handoff={{ blocked: busy, onConfirm: (c) => actions.handoff(meta.id, c.engine, c.model, c.effort) }}
            />
            <AccountPicker state={state} host={host} engine={meta.engine} account={accountOf(meta)} onManage={() => actions.go({ page: "new" })} />
            <EffortPicker models={models} value={{ engine: meta.engine, model: meta.model, effort: meta.effort }} onChange={(effort) => actions.setModel(meta.id, meta.model, effort)} />
            <PermissionPicker engine={meta.engine} value={meta.permission ?? "ask"} onChange={(p) => actions.setPermission(meta.id, p)} />
            </>
            )}
          </Composer>
        </div>
        )}
      </main>
      {!noPane && <WorkPane key={meta.id} shown={workspaceShown} connected={online} actions={actions} meta={meta} state={state} items={items} closeButton={workspaceCloseButton} onClose={closeWorkspace} file={openedFile} onOpenFile={openFile} onCloseFile={() => setOpenedFile(null)} />}
    </div>
    </FileLinks.Provider>
  );
}

/** Deleting asks for a second click; the ask lapses after a few seconds. A
 *  session in a worktree yardd made can take its worktree with it. */
function DeleteButton({ worktree, onConfirm }: { worktree: boolean; onConfirm(removeWorktree: boolean): void }) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(false), worktree ? 5000 : 3000);
    return () => clearTimeout(timer);
  }, [armed]);
  if (worktree && armed) {
    return (
      <>
        {/* A phone's bar has room for both choices only with short labels,
            and only once its other controls step aside (bar-armed in styles.css). */}
        <button className="btn-text bar-choice bar-armed" onClick={() => onConfirm(false)}>
          <span className="label-wide">Delete session</span>
          <span className="label-narrow">Delete</span>
        </button>
        <button className="btn-text bar-choice bar-armed" onClick={() => onConfirm(true)}>
          <span className="label-wide">Delete and remove worktree</span>
          <span className="label-narrow">Delete + worktree</span>
        </button>
      </>
    );
  }
  return (
    // The title ellipsizes; the button keeps its one line beside it.
    <button className="btn-text bar-choice" onClick={() => (armed ? onConfirm() : setArmed(true))}>
      {armed ? "Delete session?" : "Delete"}
    </button>
  );
}

/**
 * Comment on part of an answer: selecting text inside one agent answer shows
 * a Comment button over the selection, which adds it to the reply box as a
 * quote card with its own comment field, and marks it in the answer.
 */
function QuoteReply({ sessionId }: { sessionId: string }) {
  const [at, setAt] = useState<{ left: number; top: number; below: boolean; text: string; range: Range } | null>(null);
  useEffect(() => {
    const answerOf = (n: Node | null) => (n instanceof Element ? n : n?.parentElement)?.closest("[data-quotable]") ?? null;
    const update = () => {
      const sel = document.getSelection();
      const box = sel && !sel.isCollapsed && sel.rangeCount > 0 ? answerOf(sel.anchorNode) : null;
      const text = sel?.toString() ?? "";
      // One answer at a time: a selection that runs into a prompt or a tool call is not a quote.
      if (!box || box !== answerOf(sel!.focusNode) || !text.trim()) return setAt(null);
      const r = sel!.getRangeAt(0).getBoundingClientRect();
      // Touch screens put their own copy menu above a selection; go below it.
      const below = r.top < 48 || window.matchMedia("(hover: none)").matches;
      setAt({ left: Math.min(Math.max(r.left + r.width / 2, 48), window.innerWidth - 48), top: below ? r.bottom : r.top, below, text, range: sel!.getRangeAt(0).cloneRange() });
    };
    document.addEventListener("selectionchange", update);
    // The button is fixed to the window; the thread scrolls under it.
    window.addEventListener("scroll", update, true);
    window.addEventListener("resize", update);
    return () => {
      document.removeEventListener("selectionchange", update);
      window.removeEventListener("scroll", update, true);
      window.removeEventListener("resize", update);
    };
  }, []);
  if (!at) return null;
  return createPortal(
    <button
      className={`btn quote-reply${at.below ? " below" : ""}`}
      style={{ left: at.left, top: at.top }}
      // Keep the selection: a mousedown on a button would clear it before the click.
      onMouseDown={(e) => e.preventDefault()}
      onClick={() => {
        quoteInComposer(sessionId, at.text, at.range);
        document.getSelection()?.removeAllRanges();
        setAt(null);
      }}
    >
      <Icon d={REPLY} />
      Comment
    </button>,
    document.body,
  );
}

/**
 * Messages sent while a turn runs, as in Claude Code: the agent reads them at
 * its next step, in the same turn (OpenCode and Grok: in the next turn). Edit takes one back into the reply box;
 * Stop gives them all back. After a failed turn they go with the next message.
 */
function PendingList({ pending, busy, hint, onSteer, onEdit, onRemove }: {
  pending: PendingMessage[];
  busy: boolean;
  hint: string;
  onSteer?(id: string): void;
  onEdit(id: string): void;
  onRemove(id: string): void;
}) {
  if (pending.length === 0) return null;
  return (
    <section className="pending" aria-label="Waiting to send">
      <div className="muted small">{busy ? `Queued: ${hint}` : "Not sent: the last turn did not finish. It goes with your next message."}</div>
      {pending.map((p) => {
        const { quotes, text } = splitQuotes(p.text);
        return (
          <div className="pending-row" key={p.id}>
            <span className="pending-text">{text || quotes.map((q) => q.note || q.text).join(" ") || p.attachments?.map((a) => a.name).join(", ")}</span>
            {quotes.length > 0 && <span className="muted small">{quotes.some((q) => q.note) ? (quotes.length === 1 ? "1 comment" : `${quotes.length} comments`) : quotes.length === 1 ? "1 quote" : `${quotes.length} quotes`}</span>}
            {!!p.attachments?.length && <span className="muted small">{p.attachments.length === 1 ? "1 file" : `${p.attachments.length} files`}</span>}
            {onSteer && <button className="btn-text pending-action pending-steer" title="Send to the current turn" onClick={() => onSteer(p.id)}>Steer</button>}
            <button className="btn-text pending-action" aria-label="Edit" title="Back to the reply box (↑)" onClick={() => onEdit(p.id)}>
              Edit
            </button>
            <button className="btn-icon" aria-label="Remove from queue" title="Remove" onClick={() => onRemove(p.id)}>
              <Icon d={CLOSE} size={12} />
            </button>
          </div>
        );
      })}
    </section>
  );
}

/** `> ` lines in your own message show as quotes, not as `>` characters.
 *  Your text stays text: only the quote lines are picked out. */
function BubbleText({ text }: { text: string }) {
  // Comments on parts of answers: each part with its comment under it.
  const commented = splitComments(text);
  if (commented) {
    const rest = commented.text.replace(/^\n+/, "");
    return (
      <div className="bubble-parts">
        <ol className="bubble-comments" aria-label="Comments on parts of the answers">
          {commented.quotes.map((q, i) => (
            <li key={i} className="bubble-comment">
              {commented.quotes.length > 1 && (
                <span className="quote-card-number" aria-hidden="true">
                  {i + 1}
                </span>
              )}
              <blockquote className="bubble-quote">{q.text}</blockquote>
              {q.note && <div className="bubble-comment-note">{q.note}</div>}
            </li>
          ))}
        </ol>
        {rest && <BubbleText text={rest} />}
      </div>
    );
  }
  // Attached terminal output arrives as fenced blocks: shown as code, the
  // text around them as before.
  const blocks = fenceSegments(text);
  if (blocks.length === 1 && !blocks[0].code) return <QuotedText text={text} />;
  return (
    <div className="bubble-parts">
      {blocks.map((b, i) =>
        b.code ? (
          <pre key={i} className="bubble-code">
            {b.text}
          </pre>
        ) : (
          <QuotedText key={i} text={b.text} block />
        ),
      )}
    </div>
  );
}

function QuotedText({ text, block }: { text: string; block?: boolean }) {
  const parts = quoteSegments(text);
  if (parts.length === 1 && !parts[0].quote) return block ? <div>{text}</div> : <>{text}</>;
  return (
    <div className="bubble-parts">
      {parts.map((part, i) =>
        part.quote ? (
          <blockquote key={i} className="bubble-quote">
            {part.text}
          </blockquote>
        ) : (
          <div key={i}>{part.text}</div>
        ),
      )}
    </div>
  );
}

/** A hand-off brief can run to hundreds of lines; fold long messages. */
const FOLD_LINES = 12;

function UserBubble({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(false);
  const lines = text.split("\n");
  if (lines.length <= FOLD_LINES)
    return (
      <div className="bubble">
        <BubbleText text={text} />
      </div>
    );
  return (
    <div className="bubble">
      <BubbleText text={expanded ? text : lines.slice(0, FOLD_LINES).join("\n")} />
      <button className="btn-text bubble-toggle" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>
        {expanded ? "Show less" : `Show more (${lines.length - FOLD_LINES === 1 ? "1 line" : `${lines.length - FOLD_LINES} lines`})`}
      </button>
    </div>
  );
}

/** Seconds since `since` (ms), ticking. The tick writes the DOM directly, so a
 *  long thread does not re-render every second. */
function Elapsed({ since }: { since: number }) {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const tick = () => {
      if (ref.current) ref.current.textContent = duration(Date.now() - since);
    };
    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  }, [since]);
  return <span ref={ref} />;
}

function Working({ since, label = "Working" }: { since: number | null; label?: string }) {
  return (
    <div className="working" role="status">
      <span className="shimmer">{label}</span>
      {since != null && (
        <span className="muted small">
          for <Elapsed since={since} />
        </span>
      )}
    </div>
  );
}

function RowView({ row, render, widget }: { row: Row; render(item: Item): ReactNode; widget?(tool: ToolItem): ReactNode }) {
  if (row.type === "item") return <>{render(row.item)}</>;
  if (row.type === "work") return <WorkGroup entries={row.entries} live={row.live} />;
  if (row.type === "widget") return <>{widget?.(row.tool) ?? null}</>;
  if (row.type === "stopped") return <div className="stopped-mark muted">{row.ms != null ? `Stopped after ${duration(row.ms)}` : "Stopped"}</div>;
  return <TurnFold row={row} render={render} />;
}

/** A finished turn's work, folded above its answer. */
function TurnFold({ row, render }: { row: Extract<Row, { type: "fold" }>; render(item: Item): ReactNode }) {
  const [open, setOpen] = useState(false);
  const verb = row.status === "interrupted" ? "Stopped after" : "Worked for";
  const label = row.ms != null ? `${verb} ${duration(row.ms)}` : row.status === "interrupted" ? "Stopped" : "Worked";
  // Its messages and steps follow in place: this line only says how long.
  if (row.inline)
    return (
      <div className="fold">
        <div className="fold-row fold-row-static">
          <span>{label}</span>
          {row.summary && <span className="muted">· {row.summary}</span>}
        </div>
      </div>
    );
  return (
    <div className="fold">
      <button className="fold-row" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span>{label}</span>
        {row.summary && <span className="muted">· {row.summary}</span>}
        <span className={`tool-chevron ${open ? "open" : ""}`}>
          <Icon d={CHEVRON_RIGHT} size={12} />
        </span>
      </button>
      {open && (
        <div className="fold-body">
          {row.rows.map((r) => (
            <RowView key={r.key} row={r} render={render} />
          ))}
        </div>
      )}
    </div>
  );
}

/** Tool calls and thinking between messages. While live, only the latest
 *  step shows; once done, one summary line that opens the list. */
function WorkGroup({ entries, live }: { entries: WorkItem[]; live: boolean }) {
  const [open, setOpen] = useState(false);
  if (entries.length === 1) return <WorkEntry entry={entries[0]} />;
  if (live && !open) {
    return (
      <div className="work">
        <button className="work-more" onClick={() => setOpen(true)}>
          {entries.length - 1} earlier {entries.length === 2 ? "step" : "steps"}
        </button>
        <WorkEntry entry={entries[entries.length - 1]} />
      </div>
    );
  }
  return (
    <div className="work">
      <button className="work-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span>{summarize(entries) || "Worked"}</span>
        <span className={`tool-chevron ${open ? "open" : ""}`}>
          <Icon d={CHEVRON_RIGHT} size={12} />
        </span>
      </button>
      {open && (
        <div className="work-list">
          {entries.map((e) => (
            <WorkEntry key={e.key} entry={e} />
          ))}
        </div>
      )}
    </div>
  );
}

function WorkEntry({ entry }: { entry: WorkItem }) {
  const [open, setOpen] = useState(false);
  if (entry.kind === "reasoning") {
    const line = thoughtLine(entry.text);
    return (
      <div className="tool">
        <button className="tool-row" aria-expanded={open} onClick={() => setOpen(!open)}>
          <span className={`tool-chevron ${open ? "open" : ""}`}>
            <Icon d={CHEVRON_RIGHT} size={12} />
          </span>
          <span className={entry.streaming ? "shimmer" : ""}>{entry.streaming ? "Thinking" : "Thought"}</span>
          {line && <span className="thought-line">{line}</span>}
        </button>
        {open && (
          <div className="thought">
            <Markdown text={entry.text} />
          </div>
        )}
      </div>
    );
  }
  if (entry.agent || entry.steps?.length) return <AgentEntry entry={entry} />;
  const expandable = !!entry.output;
  const command = stepCommand(entry);
  return (
    <div className="tool">
      <div className="tool-line">
        <button className="tool-row" aria-expanded={open} disabled={!expandable} onClick={() => setOpen(!open)}>
          <span className={`tool-chevron ${open ? "open" : ""} ${expandable ? "" : "hidden"}`}>
            <Icon d={CHEVRON_RIGHT} size={12} />
          </span>
          {/* Engines title a command "Ran" and an edit "Edited" from the start;
              while its approval is open or after a denial it never happened. */}
          <span>{toolLabel(entry)}</span>
          {entry.detail && <code>{entry.detail}</code>}
          {entry.state === "running" && entry.asked !== "waiting" && <span className="pulse" aria-label="running" />}
          {entry.asked === "waiting" && toolLabel(entry) === entry.title && <span className="muted">· waiting</span>}
          {didNotRun(entry) ? (
            <span className="muted">· {entry.asked === "denied" ? "denied" : "not answered"}</span>
          ) : (
            entry.state === "failed" && <span className="bad">· failed</span>
          )}
        </button>
        {command && <CopyButton text={command} label="Copy command" />}
      </div>
      {open && entry.output && <CopyBlock text={entry.output} className="tool-output" label="Copy output" />}
    </div>
  );
}

/** Preformatted text with Copy beside it: code, commands' output. */
export function CopyBlock({ text, className, label = "Copy" }: { text: string; className: string; label?: string }) {
  return (
    <div className={`copy-block ${className}-block`}>
      <pre className={className}>{text}</pre>
      <CopyButton text={text} label={label} className="copy-corner" />
    </div>
  );
}

/** A subagent: one line, collapsed. Open, the steps it took and its report. */
function AgentEntry({ entry, card = false }: { entry: ToolItem; card?: boolean }) {
  const [open, setOpen] = useState(false);
  const steps = entry.steps ?? [];
  const expandable = steps.length > 0 || !!entry.output;
  const latest = steps[steps.length - 1];
  const parts = [
    entry.state === "running" ? "running" : entry.state === "failed" ? "failed" : "done",
    entry.state !== "running" && entry.ms != null ? duration(entry.ms) : null,
    steps.length ? `${steps.length} ${steps.length === 1 ? "step" : "steps"}` : null,
  ].filter(Boolean);
  const chevron = (
    <span className={`tool-chevron ${open ? "open" : ""} ${expandable ? "" : "hidden"}`}>
      <Icon d={CHEVRON_RIGHT} size={12} />
    </span>
  );
  const status = (
    <>
      {entry.state === "running" && <span className="pulse" aria-label="running" />}
      <span className={entry.state === "failed" ? "bad small" : "muted small"}>{parts.join(" · ")}</span>
    </>
  );
  const latestLine = entry.state === "running" && latest && !open ? <code>{latest.detail}</code> : null;
  return (
    <div className={`tool agent ${card ? "subagent" : ""}`}>
      {card ? (
        // The pane lists only subagents, so the task leads and the word "Agent"
        // goes; type, state and the latest step line up under it in every card.
        <button className="tool-row subagent-head" aria-expanded={open} disabled={!expandable} onClick={() => setOpen(!open)}>
          {chevron}
          <span className="subagent-title">{entry.detail || "Agent"}</span>
          <span className="subagent-meta">
            {entry.agent && <span className="agent-type">{entry.agent}</span>}
            {status}
          </span>
          {latestLine && <span className="subagent-latest">{latestLine}</span>}
        </button>
      ) : (
        <button className="tool-row" aria-expanded={open} disabled={!expandable} onClick={() => setOpen(!open)}>
          {chevron}
          <span>Agent</span>
          {entry.detail && <span className="agent-what">{entry.detail}</span>}
          {entry.agent && <span className="agent-type">{entry.agent}</span>}
          {status}
          {latestLine}
        </button>
      )}
      {open && (
        <div className="agent-body">
          {steps.length > 0 && (
            <div className="work-list">
              {steps.map((s) => (
                <WorkEntry key={s.key} entry={s} />
              ))}
            </div>
          )}
          {entry.output && <CopyBlock text={entry.output} className="tool-output" label="Copy output" />}
        </div>
      )}
    </div>
  );
}

type OnDecide = (requestId: string, decision: "allow" | "allow_always" | "deny", answers?: Record<string, string>) => void;

export function ApprovalCard({ item, onDecide, asker }: {
  item: Extract<Item, {kind: "approval"}>;
  onDecide: OnDecide;
  /** Who asks a question: "Claude", "Codex". */
  asker?: string;
}) {
  if (item.questions?.length) return <QuestionCard item={item} questions={item.questions} onDecide={onDecide} asker={asker ?? "The agent"} />;
  const slackAccess = item.approvalKind === "slackAccess";
  return (
        <div className="approval">
          <div className="approval-title">{slackAccess ? "Approve tool access for this Slack request?" : item.approvalKind === "command" ? "Allow this command?" : "Allow this?"}</div>
          <div className="approval-what-row">
            {slackAccess ? <span className="approval-what">{item.title}</span> : <code className="approval-what">{item.title}</code>}
            {item.approvalKind === "command" && item.title.trim() && <CopyButton text={item.title} label="Copy command" />}
          </div>
          {item.detail && <div className="muted small">{item.detail}</div>}
          {item.decision === null && !item.unanswered ? (
            <div className="row">
              <button className="btn-primary" disabled={item.sending} onClick={() => onDecide(item.requestId, "allow")}>
                {slackAccess ? "Approve once" : "Allow once"}
              </button>
              <button className="btn" disabled={item.sending} onClick={() => onDecide(item.requestId, "allow_always")}>
                {slackAccess ? `Always allow ${item.detail}` : "Always allow"}
              </button>
              <button className="btn-text" disabled={item.sending} onClick={() => onDecide(item.requestId, "deny")}>
                Deny
              </button>
            </div>
          ) : (
            <div className="muted small">
              {item.decision === null
                ? "Not answered before the turn ended"
                : item.decision === "deny"
                  ? "Denied"
                  : item.decision === "allow_always"
                    ? "Always allowed"
                    : "Allowed"}
            </div>
          )}
        </div>
);
}

/** The agent asks the user to choose (Claude's AskUserQuestion, Codex's
 *  request_user_input): choose, or type your own, then Send. Skip tells it the
 *  user chose not to answer. The agent's text: React text only. */
function QuestionCard({ item, questions, onDecide, asker }: {
  item: Extract<Item, {kind: "approval"}>;
  questions: Question[];
  onDecide: OnDecide;
  asker: string;
}) {
  const [picks, setPicks] = useState<Record<string, Pick>>({});
  const open = item.decision === null && !item.unanswered;
  const answers = answersFrom(questions, picks);
  const set = (q: Question, pick: Pick) => setPicks((p) => ({ ...p, [q.question]: pick }));
  return (
    <div className="approval question-card">
      <div className="approval-title">{questions.length > 1 ? `${asker} has ${questions.length} questions` : `${asker} has a question`}</div>
      {questions.map((q, qi) => {
        const pick = picks[q.question] ?? noPick;
        const given = item.answers?.[q.question];
        return (
          <div key={qi} className="question" role="group" aria-label={q.question}>
            <div className="question-head">
              {q.header && <span className="question-chip">{q.header}</span>}
              <span className="question-text">{q.question}</span>
            </div>
            {open ? (
              <>
                {q.options.length > 0 && <div className="question-options" role={q.multiSelect ? "group" : "radiogroup"} aria-label={q.question}>
                  {q.options.map((o) => (
                    <button
                      key={o.label}
                      type="button"
                      role={q.multiSelect ? "checkbox" : "radio"}
                      aria-checked={pick.labels.includes(o.label)}
                      className="question-option"
                      disabled={item.sending}
                      onClick={() => set(q, toggle(q, pick, o.label))}
                    >
                      <span className={`question-mark ${q.multiSelect ? "question-mark-box" : ""}`} aria-hidden="true" />
                      <span className="question-option-words">
                        <span className="question-option-label">{o.label}</span>
                        {o.description && <span className="muted small">{o.description}</span>}
                      </span>
                    </button>
                  ))}
                </div>}
                {q.other !== false && <input
                  className="field question-other"
                  type={q.secret ? "password" : "text"}
                  autoComplete="off"
                  placeholder={!q.options.length ? "Your answer" : q.multiSelect ? "Something else (optional)" : "Something else"}
                  aria-label={q.options.length ? `Something else: ${q.question}` : `Answer: ${q.question}`}
                  value={pick.other}
                  disabled={item.sending}
                  onChange={(e) => set(q, typeOther(q, pick, e.target.value))}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && answers && !item.sending) onDecide(item.requestId, "allow", answers);
                  }}
                />}
                {q.multiSelect && <div className="muted small">Choose any that apply.</div>}
              </>
            ) : (
              given && <div className="question-answer">{given}</div>
            )}
          </div>
        );
      })}
      {open ? (
        <div className="row">
          <button className="btn-primary" disabled={!answers || item.sending} onClick={() => answers && onDecide(item.requestId, "allow", answers)}>
            {questions.length > 1 ? "Send answers" : "Send answer"}
          </button>
          <button className="btn-text" disabled={item.sending} onClick={() => onDecide(item.requestId, "deny")}>
            Skip
          </button>
        </div>
      ) : (
        <div className="muted small">
          {item.decision === null
            ? "Not answered before the turn ended"
            : item.decision === "deny"
              ? "Skipped"
              : item.answers && Object.keys(item.answers).length
                ? "Answered"
                : "Sent without answers"}
        </div>
      )}
    </div>
  );
}

export function QueueNotice({ reason, starting, onStart }: {reason: string; starting: boolean; onStart(): void}) {
  return (
    <div className="queued-line">
      <span>Queued: {reason}.</span>
      <button className="btn" disabled={starting} onClick={onStart}>Start anyway</button>
    </div>
  );
}

function ItemView(props: {
  item: Item;
  /** For a hand-off note: the session on the other side. */
  link?: string;
  onOpen(sessionId: string): void;
  /** The session is still waiting in the queue. */
  queued: boolean;
  /** Start anyway was clicked and yardd has not answered yet. */
  starting: boolean;
  loadAttachment(a: Attachment): Promise<string>;
  onStart(): void;
  onDecide: OnDecide;
  /** Who asks a question card's questions: "Claude", "Codex". */
  asker?: string;
  /** Whose face goes on a message from another agent. */
  agents: Agent[];
}) {
  const { item, onDecide } = props;
  switch (item.kind) {
    case "user": {
      const origin = originLabel(item.origin);
      if (!item.attachments?.length && !origin) return <UserBubble text={item.text} />;
      const from = origin && props.agents.find((a) => a.id === origin.agentId);
      return (
        <div className="user-message">
          {origin && (
            <div className={`origin-badge origin-${item.origin!.kind}`} title={item.origin!.kind === "slack" ? "Its answer is posted back to the Slack thread." : undefined}>
              {from ? <AgentAvatar agent={from} size="xs" /> : <span className="origin-dot" aria-hidden="true" />}
              {origin.text}
            </div>
          )}
          {item.attachments?.length ? (
            <div className="attachments attachments-sent">
            {item.attachments.map((a) => (
              <SentAttachment key={a.id} attachment={a} load={props.loadAttachment} />
            ))}
            </div>
          ) : null}
          {item.text && <UserBubble text={item.text} />}
        </div>
      );
    }
    case "note":
      return (
        <div className="note">
          {item.text}
          {props.link && (
            <button className="btn-text note-link" onClick={() => props.onOpen(props.link!)}>
              Open session
            </button>
          )}
        </div>
      );
    case "assistant":
      return <Markdown text={item.text} quotable />;
    case "tool":
    case "reasoning":
      return <WorkEntry entry={item} />;
    case "turn":
      return null;
    case "diff":
      return (
        <>
          {item.files.map((f, i) => (
            <FileDiff key={i} file={f} />
          ))}
        </>
      );
    case "approval":
      return <ApprovalCard item={item} onDecide={onDecide} asker={props.asker} />;
    case "error":
      return <div className="error-line">{item.message}</div>;
    case "queued":
      return props.queued ? <QueueNotice reason={item.reason} starting={props.starting} onStart={props.onStart} /> : <div className="queued-line">Was queued: {item.reason}.</div>;
  }
}

/* ---------- diffs ---------- */

function DiffLines({ diff }: { diff: string }) {
  const lines = diff.replace(/\n$/, "").split("\n");
  return (
    <pre className="diff">
      {lines.map((line, i) => {
        const cls = line.startsWith("@@") ? "hunk" : line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : "ctx";
        return (
          <span key={i} className={`dl ${cls}`}>
            {line || " "}
          </span>
        );
      })}
    </pre>
  );
}

function FileDiff({ file }: { file: ChangedFile }) {
  return (
    <div className="card">
      <div className="card-head">
        <code>{file.path}</code>
      </div>
      {file.diff ? <DiffLines diff={file.diff} /> : <div className="muted small pad">No text diff.</div>}
    </div>
  );
}

function WorkPane({ shown, connected, actions, meta, state, items, closeButton, onClose, file, onOpenFile, onCloseFile }: {
  shown: boolean;
  connected: boolean;
  actions: Actions;
  meta: SessionMeta;
  state: State;
  items: Item[];
  closeButton: RefObject<HTMLButtonElement | null>;
  onClose(): void;
  /** A file a link opened: its tab shows while it is open. */
  file: { path: string; nonce: number } | null;
  onOpenFile(path: string): Promise<string | null>;
  onCloseFile(): void;
}) {
  const [paneW, setPaneW] = useWidth("yard.paneWidth", PANE_W);
  const [tab, setTab] = useState<"terminal" | "agents" | "procs" | "file">("terminal");
  useEffect(() => {
    if (file) setTab("file");
    else setTab((t) => (t === "file" ? "terminal" : t));
  }, [file?.path, file?.nonce]);
  const [terminalUsed, setTerminalUsed] = useState(shown);
  const agents = useMemo(() => tab === "agents" ? sessionSubagents(items) : [], [tab, items]);
  useEffect(() => {
    if (shown && tab === "terminal") setTerminalUsed(true);
  }, [shown, tab]);
  return (
    <section id="session-workspace" className="pane" aria-label="Workspace" hidden={!shown} style={{ "--pane-w": `${paneW}px` } as CSSProperties}>
      <Resizer width={paneW} min={300} max={Math.max(300, window.innerWidth - 640)} initial={PANE_W} dir={-1} cssVar="--pane-w" label="Resize the workspace pane" onWidth={setPaneW} />
      <header className="bar tabs workspace-tabs">
        <button ref={closeButton} className="btn-icon workspace-close" aria-label="Back to conversation" onClick={onClose}><Icon d={CLOSE} /></button>
        <button className={`tab ${tab === "terminal" ? "active" : ""}`} aria-pressed={tab === "terminal"} onClick={() => setTab("terminal")}>Terminal</button>
        <button className={`tab ${tab === "agents" ? "active" : ""}`} aria-pressed={tab === "agents"} onClick={() => setTab("agents")}>Subagents</button>
        <button className={`tab ${tab === "procs" ? "active" : ""}`} aria-pressed={tab === "procs"} onClick={() => setTab("procs")}>Processes</button>
        {file && (
          <span className={`file-tab ${tab === "file" ? "active" : ""}`}>
            <button className={`tab ${tab === "file" ? "active" : ""}`} aria-pressed={tab === "file"} aria-label={fileLabel(file.path)} title={file.path} onClick={() => setTab("file")}>
              <Icon d={FILE} size={13} />
              <span className="file-tab-name">{fileLabel(file.path)}</span>
            </button>
            <button className="btn-icon file-tab-close" aria-label={`Close ${fileLabel(file.path)}`} onClick={onCloseFile}>
              <Icon d={CLOSE} size={12} />
            </button>
          </span>
        )}
        <span className="spacer" />
        {tab === "procs" && <button className="btn-icon" aria-label="Refresh processes" onClick={() => actions.loadProcs(meta.id)}><Icon d={REFRESH} /></button>}
      </header>
      {terminalUsed && <TerminalPane sessionId={meta.id} connected={connected} visible={shown && tab === "terminal"} actions={actions} />}
      {shown && tab === "agents" && (
        <section aria-label="Session subagents" className="pane-body subagents-pane">
          {agents.length ? agents.map((agent) => <AgentEntry key={agent.key} entry={agent} card />) : <p className="muted">No subagents in this session.</p>}
        </section>
      )}
      {shown && tab === "procs" && <ProcPane meta={meta} state={state} actions={actions} visible />}
      {file && tab === "file" && <FileView sessionId={meta.id} path={file.path} nonce={file.nonce} home={homeOn(state, hostOf(meta))} actions={actions} onOpen={onOpenFile} />}
    </section>
  );
}

/* ---------- markdown (rendered as elements, never as HTML) ---------- */

const WEB_URL = /https?:\/\/(?:[^\s<>()\[\]{}"`']|\([^\s<>()]*\))+/;
const INLINE = new RegExp(`(${[/\[`?[^\]`]+`?\]\([^)]+\)/.source, /`[^`]+`/.source, /\*\*[^*]+\*\*/.source, PR_URL_SOURCE, WEB_URL.source].join("|")})`, "gi");

/** A link to a file on the session's host. Clicking opens it in the
 *  workspace; a failure to download one where there is no workspace shows
 *  next to the link. */
function FileLink({ path, label }: { path: string; label: string }) {
  const open = useContext(FileLinks);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!open) return <code>{label}</code>;
  return (
    <>
      <button
        type="button"
        className="file-link"
        title={`Open ${path}`}
        aria-busy={busy || undefined}
        onClick={async () => {
          if (busy) return;
          setBusy(true);
          setError(null);
          setError(await open(path));
          setBusy(false);
        }}
      >
        <Icon d={FILE} size={12} />
        {label}
      </button>
      {error && (
        <span className="file-link-error" role="alert">
          {error}
        </span>
      )}
    </>
  );
}

function webHref(raw: string): string | null {
  if (!/^https?:\/\/[^\s\\]+$/i.test(raw)) return null;
  try {
    const url = new URL(raw);
    return url.username || url.password ? null : url.href;
  } catch {
    return null;
  }
}

function inline(text: string): ReactNode[] {
  return text.split(INLINE).map((part, i) => {
    if (i % 2 === 0) return part;
    const link = part.match(/^\[`?([^\]`]+)`?\]\(([^)]+)\)$/);
    const destination = link?.[2] ?? part.replace(/[.,;:!?]+$/, "");
    const href = webHref(destination);
    if (href) {
      const pull = prLink(href);
      const label = link?.[1] ?? (pull ? shortLabel(pull) : destination);
      return (
        <Fragment key={i}>
          {pull ? <PrLink href={href}>{label}</PrLink> : <a href={href} target="_blank" rel="noopener noreferrer">{label}</a>}
          {!link && part.slice(destination.length)}
        </Fragment>
      );
    }
    const file = link && fileTarget(link[2]);
    if (file) return <FileLink key={i} path={file} label={link[1]} />;
    if (link) return <code key={i}>{link[1]}</code>;
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) return <code key={i}>{part.slice(1, -1)}</code>;
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) return <strong key={i}>{inline(part.slice(2, -2))}</strong>;
    return part;
  });
}

/** Wide tables scroll sideways in their own box, not the whole thread. */
function MdTableView({ table }: { table: MdTable }) {
  const cls = (k: number) => (table.align[k] ? `align-${table.align[k]}` : undefined);
  return (
    <div className="md-table">
      <table>
        <thead>
          <tr>
            {table.header.map((c, k) => (
              <th key={k} className={cls(k)}>
                {inline(c)}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {table.rows.map((row, r) => (
            <tr key={r}>
              {row.map((c, k) => (
                <td key={k} className={cls(k)}>
                  {inline(c)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function Markdown({ text, quotable }: { text: string; quotable?: boolean }) {
  const blocks = fencedBlocks(text);
  return (
    <div className="prose" data-quotable={quotable ? "" : undefined}>
      {blocks.map((block, i) => {
        if (block.kind === "code") {
          if (block.language === "mermaid") return <MermaidDiagram key={i} source={block.body} complete={block.complete} />;
          return <CopyBlock key={i} text={block.body} className="code" label="Copy code" />;
        }
        return block.text
          .split(/\n{2,}/)
          .filter((p) => p.trim())
          .map((para, j) => {
            const lines = para.replace(/^\n+|\n+$/g, "").split("\n");
            return blockSegments(lines).map((seg, s) => {
              const key = `${i}-${j}-${s}`;
              switch (seg.kind) {
                case "table":
                  return <MdTableView key={key} table={seg.table} />;
                case "heading":
                  return (
                    <h3 key={key} data-level={seg.level}>
                      {inline(seg.text)}
                    </h3>
                  );
                case "quote":
                  return (
                    <blockquote key={key}>
                      {seg.lines.map((l, k) => (
                        <span key={k}>
                          {k > 0 && <br />}
                          {inline(l)}
                        </span>
                      ))}
                    </blockquote>
                  );
                case "ul":
                case "ol": {
                  const List = seg.kind;
                  return (
                    <List key={key} start={seg.kind === "ol" ? seg.start : undefined}>
                      {seg.items.map((item, k) => (
                        <li key={k}>{inline(item)}</li>
                      ))}
                    </List>
                  );
                }
                default:
                  return (
                    <p key={key}>
                      {seg.lines.map((l, k) => (
                        <span key={k}>
                          {k > 0 && <br />}
                          {inline(l)}
                        </span>
                      ))}
                    </p>
                  );
              }
            });
          });
      })}
    </div>
  );
}

/* ---------- hosts ---------- */

function Sparkline({ values }: { values: number[] }) {
  if (values.length < 2) return <div className="muted small">Collecting CPU history.</div>;
  const points = values
    .map((v, i) => `${((i / (values.length - 1)) * 300).toFixed(1)},${(35 - (Math.min(100, v) / 100) * 34).toFixed(1)}`)
    .join(" ");
  return (
    <svg role="img" aria-label="CPU over the last hour" viewBox="0 0 300 36" preserveAspectRatio="none" className="spark">
      <polyline points={points} fill="none" stroke="currentColor" strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

/** `8` or `0.2`, without a trailing `.0`. */
const gbLimit = (bytes: number) => String(Number((bytes / 1024 ** 3).toFixed(1)));

/** The queue policy belongs with system capacity, not the session list. */
function QueuePolicy({ host }: { host: Host }) {
  const limits = host.limits;
  if (!limits) return null;
  return (
    <p className="muted small">
      {limits.forced
        ? "New sessions are queued: pressure is forced on (YARD_FORCE_PRESSURE=1)."
        : limits.minFreeBytes > 0
          ? `New sessions are queued while load is above ${host.cores} or less than ${gbLimit(limits.minFreeBytes)} GB of memory is free.`
          : `New sessions are queued while load is above ${host.cores}.`}
    </p>
  );
}

type HostState = "ok" | "busy" | "down" | "connecting";
function hostState(state: State, id: string): HostState {
  const entry = state.hosts.find((h) => h.id === id);
  if (id !== "local" && !hostOnline(state, id)) return entry?.status === "connecting" ? "connecting" : "down";
  const stats = state.host[id];
  return stats && hostPressure(stats) ? "busy" : "ok";
}
const HOST_STATE_LABEL: Record<HostState, string> = { ok: "Healthy", busy: "Under pressure", down: "Offline", connecting: "Connecting" };

/** yardd passes on what ssh or the socket said; turn the usual ones into
 *  what to do about it. The raw text stays available under Details. */
function explainHostError(entry: HostEntry): { title: string; hint?: ReactNode; install?: boolean } {
  const err = entry.error ?? "";
  const dest = entry.destination ?? "";
  const machine = dest.replace(/^.*@/, "");
  if (/Connection lost/i.test(err)) return { title: "Connection lost. Reconnecting…" };
  if (entry.kind === "ssh" && entry.noDaemon)
    return {
      title: `SSH works, but Switchyard isn't running on ${machine} yet.`,
      hint: "Install it from here: it downloads Switchyard for that machine and runs it as a service in your user account. Nothing else on the machine changes.",
      install: true,
    };
  if (entry.kind === "ssh") {
    if (/Could not resolve hostname/i.test(err))
      return { title: `Can't find ${machine}.`, hint: "Check the name. A Tailscale machine also answers to its full name, like box.tailnet.ts.net." };
    if (/Permission denied/i.test(err))
      return { title: "SSH refused the login.", hint: <>Make <code>ssh {dest}</code> work in Terminal without a password prompt (a key or Tailscale SSH), then wait for the retry.</> };
    if (/Host key verification failed/i.test(err))
      return { title: "The host key is not trusted yet.", hint: <>Run <code>ssh {dest}</code> once in Terminal and accept the key.</> };
    if (/timed out|No route to host|Network is unreachable/i.test(err) && !/os error/i.test(err))
      return { title: `Can't reach ${machine}.`, hint: "Is it on, and on the same network or tailnet?" };
  }
  return { title: err || "Not connected." };
}

/** An engine that cannot run here yet: not installed, or not logged in. */
function EngineNotice({ state, host, engine, account = DEFAULT_ACCOUNT, onLogin }: { state: State; host: string; engine: Engine; account?: string; onLogin(): void }) {
  const st = account === DEFAULT_ACCOUNT ? state.engines[host]?.[engine] : state.engines[host]?.accounts?.[account];
  if (!st || (st.installed && st.loggedIn !== false)) return null;
  const machine = hostName(state, host);
  const which = account === DEFAULT_ACCOUNT ? ENGINE_LABEL[engine] : `${ENGINE_LABEL[engine]} (${accountName(accountsOn(state, host), engine, account)})`;
  return (
    <div className="engine-notice" role="status">
      <span className="dot host-dot-busy" aria-hidden="true" />
      {st.installed ? (
        <>
          <span className="engine-notice-text">
            {which} isn't logged in on {machine}.
          </span>
          <span className="spacer" />
          <button className="btn-primary" onClick={onLogin}>
            Log in
          </button>
        </>
      ) : (
        <span className="engine-notice-text">
          {ENGINE_LABEL[engine]} isn't installed on {machine}. Install it there with <code>{ENGINE_INSTALL[engine]}</code>.
        </span>
      )}
    </div>
  );
}

/** Runs an engine's login on a host. Codex: open a link and enter a code
 *  there. Claude Code: open a link, sign in, paste the code it shows here. */
function LoginDialog(props: { state: State; actions: Actions; host: string; engine: Engine; account?: string; onClose(): void }) {
  const { state, actions, host, engine, onClose } = props;
  const account = props.account ?? DEFAULT_ACCOUNT;
  // Which login this is, when the engine has several: "Claude Code (Work)".
  const which = account === DEFAULT_ACCOUNT ? ENGINE_LABEL[engine] : `${ENGINE_LABEL[engine]} (${accountName(accountsOn(state, host), engine, account)})`;
  const current = state.logins[loginKey(host, engine, account)];
  // Whatever was there before this dialog opened belongs to an older login.
  const before = useRef(current);
  const login = current === before.current ? undefined : current;
  const [code, setCode] = useState("");
  const [copied, setCopied] = useState(false);
  // The terminal login shown when Check login was pressed: a newer one means
  // the check came back still logged out, which otherwise looks like nothing happened.
  const [checkedFrom, setCheckedFrom] = useState<LoginState | undefined>();
  const machine = hostName(state, host);
  const finished = login?.state === "done";
  useEffect(() => {
    actions.login(host, engine, account);
  }, []);
  useEffect(() => {
    if (!finished) return;
    const timer = setTimeout(onClose, 1500);
    return () => clearTimeout(timer);
  }, [finished]);
  const close = () => {
    if (!finished && login?.state !== "failed") actions.loginCancel(host, engine, account);
    onClose();
  };
  useEscape(true, close);
  const retry = () => {
    before.current = state.logins[loginKey(host, engine, account)];
    setCode("");
    actions.login(host, engine, account);
  };
  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* the code stays selectable */
    }
  };

  let body: ReactNode;
  if (!login || login.state === "starting" || login.state === "cancelled") {
    body = <p className="muted">Starting the login on {machine}…</p>;
  } else if (login.state === "terminal") {
    body = (
      <>
        <p>Run this in a terminal on {machine}, then choose Log in with Google:</p>
        <div className="login-code login-command">
          <code>{login.command}</code>
          <button className="btn" onClick={() => copy(login.command ?? "gemini")}>{copied ? "Copied" : "Copy"}</button>
        </div>
        {login.instructions && <p className="muted small">{login.instructions}</p>}
        <p className="muted small">Gemini CLI keeps the login on that machine. After signing in, check it here.</p>
        {checkedFrom && checkedFrom !== login && (
          <p className="small warn" role="status">
            Still not logged in on {machine}. Finish signing in there, then check again.
          </p>
        )}
        <button
          className="btn-primary"
          onClick={() => {
            setCheckedFrom(login);
            actions.login(host, engine, account);
          }}
        >
          Check login
        </button>
      </>
    );
  } else if (login.state === "providers") {
    body = <ProviderPicker providers={login.providers ?? []} onPick={(provider, method) => actions.loginChoose(host, provider, method, account)} />;
  } else if (login.state === "key") {
    body = (
      <form
        className="login-key"
        onSubmit={(e) => {
          e.preventDefault();
          if (code.trim()) actions.loginCode(host, engine, code.trim(), account);
        }}
      >
        <label className="muted small" htmlFor="login-key">
          {login.label || "API key"} for {login.provider}. OpenCode keeps it in its own files on {machine}; Switchyard does not.
        </label>
        <div className="login-code">
          <input
            id="login-key"
            autoFocus
            type="password"
            value={code}
            onChange={(e) => setCode(e.target.value)}
            placeholder="Paste the key"
            spellCheck={false}
            autoComplete="off"
          />
          <button className="btn-primary" type="submit" disabled={!code.trim()}>
            Save
          </button>
        </div>
      </form>
    );
  } else if (login.state === "checking" && engine === "opencode") {
    body = <p className="muted">Checking with OpenCode on {machine}…</p>;
  } else if (login.state === "code") {
    body = (
      <>
        <ol className="login-steps">
          <li>
            Open{" "}
            <a href={login.url} target="_blank" rel="noreferrer">
              {login.url}
            </a>{" "}
            {engine === "opencode" ? `and sign in to ${login.provider ?? "the provider"}.` : `and sign in with your ${ENGINE_ACCOUNT[engine] ?? ENGINE_NAME[engine]} account.`}
          </li>
          {login.code ? <li>Enter this code:</li> : login.instructions ? <li>{login.instructions}</li> : null}
        </ol>
        {login.code && (
          <div className="login-code">
            <code>{login.code}</code>
            <button className="btn" onClick={() => copy(login.code ?? "")}>
              {copied ? "Copied" : "Copy"}
            </button>
          </div>
        )}
        <p className="muted small">
          {engine === "codex" ? "This closes by itself once you've entered the code. It expires after 15 minutes." : "This closes by itself once you've signed in."}
        </p>
      </>
    );
  } else if (login.state === "url" || login.state === "checking") {
    const checking = login.state === "checking";
    body = (
      <>
        <ol className="login-steps">
          <li>
            <a className="btn-primary login-open" href={login.url} target="_blank" rel="noreferrer">
              Open the sign-in page
            </a>
          </li>
          <li>{engine === "opencode" ? login.instructions || "Sign in and copy the code the page shows." : "Sign in with your Claude account and copy the code it shows."}</li>
          <li>Paste it here:</li>
        </ol>
        <form
          className="login-code"
          onSubmit={(e) => {
            e.preventDefault();
            if (code.trim()) actions.loginCode(host, engine, code.trim(), account);
          }}
        >
          <input
            autoFocus
            value={code}
            onChange={(e) => setCode(e.target.value)}
            placeholder="Code from the sign-in page"
            spellCheck={false}
            autoComplete="off"
            disabled={checking}
            aria-label="Code from the sign-in page"
          />
          <button className="btn-primary" type="submit" disabled={checking || !code.trim()}>
            {checking ? "Checking…" : "Log in"}
          </button>
        </form>
      </>
    );
  } else if (login.state === "done") {
    body = (
      <p className="login-done">
        <Icon d={CHECK} /> {engine === "opencode" ? `OpenCode can use the provider on ${machine}.` : `${which} is logged in on ${machine}.`}
      </p>
    );
  } else {
    body = (
      <>
        <p className="bad">{login.error || "The login did not finish."}</p>
        <div className="row">
          <button className="btn-primary" onClick={retry}>
            Try again
          </button>
        </div>
      </>
    );
  }

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="modal" role="dialog" aria-modal="true" aria-label={`Log in to ${which}`}>
        <header className="modal-head">
          <h2>
            Log in to {which} on {machine}
          </h2>
          <span className="spacer" />
          <button className="btn-icon" aria-label="Close" onClick={close}>
            <Icon d={CLOSE} size={14} />
          </button>
        </header>
        {body}
      </div>
    </div>
  );
}

/** OpenCode's providers, connected ones first, and the ways into each one
 *  that work from the app. Picking a provider with one way in takes it. */
function ProviderPicker({ providers, onPick }: { providers: LoginProvider[]; onPick(provider: string, method: number): void }) {
  const [filter, setFilter] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
  const rows = providers.filter((p) => words.every((w) => `${p.name} ${p.id}`.toLowerCase().includes(w))).slice(0, 80);
  const pick = (p: LoginProvider) => (p.methods.length === 1 ? onPick(p.id, p.methods[0].index) : setOpen(open === p.id ? null : p.id));
  return (
    <>
      <p className="muted small">Pick a provider for OpenCode to use. Its key or sign-in stays in OpenCode's own files.</p>
      <input
        className="import-filter provider-filter"
        autoFocus
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        placeholder="Filter providers"
        aria-label="Filter providers"
        spellCheck={false}
      />
      {rows.length === 0 ? (
        <p className="muted">No provider matches.</p>
      ) : (
        <ul className="provider-list">
          {rows.map((p) => (
            <li key={p.id}>
              <button className="picker-item provider-row" aria-expanded={p.methods.length > 1 ? open === p.id : undefined} onClick={() => pick(p)}>
                <span>{p.name}</span>
                <span className="muted small">{p.connected ? "added" : p.methods.length > 1 ? `${p.methods.length} ways in` : p.methods[0]?.label}</span>
              </button>
              {open === p.id && (
                <div className="provider-methods">
                  {p.methods.map((m) => (
                    <button key={m.index} className="btn" onClick={() => onPick(p.id, m.index)}>
                      {m.label}
                    </button>
                  ))}
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

/** A Claude Code session touched this recently may still be open somewhere. */
const RECENT_MS = 10 * 60_000;

/** "5 min ago": when a Claude Code session was last used. */
function ago(ms: number): string {
  const min = Math.round((Date.now() - ms) / 60000);
  if (min < 1) return "just now";
  if (min < 60) return `${min} min ago`;
  if (min < 48 * 60) return `${Math.round(min / 60)} h ago`;
  if (min < 30 * 24 * 60) return `${Math.round(min / 1440)} d ago`;
  return new Date(ms).toLocaleDateString();
}

/**
 * Import a Claude Code session run outside Switchyard: the host's sessions
 * (from the Agent SDK) newest first; importing one opens it here, in its own
 * folder, and its next message continues the same conversation.
 */
function ImportDialog({ state, actions, onClose }: { state: State; actions: Actions; onClose(): void }) {
  const [host, setHost] = useState("local");
  const [filter, setFilter] = useState("");
  const [pending, setPending] = useState<string | null>(null);
  const list = state.claudeSessions[host];
  const machine = hostName(state, host);
  const close = () => {
    actions.clearImportError();
    onClose();
  };
  useEscape(true, close);
  useEffect(() => {
    actions.clearImportError();
    setPending(null);
    actions.listClaudeSessions(host);
  }, [host]);
  // The import's `created` reply opens the session: the dialog's work is done.
  const opened = state.route.page === "session" ? state.route.id : null;
  const first = useRef(opened);
  useEffect(() => {
    if (pending && opened && opened !== first.current) onClose();
  }, [opened]);
  useEffect(() => {
    if (state.importError) setPending(null);
  }, [state.importError]);
  const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
  const rows = (list?.sessions ?? []).filter((s) => words.every((w) => `${s.title} ${s.cwd ?? ""} ${s.gitBranch ?? ""}`.toLowerCase().includes(w)));
  const open = (id: string) => {
    actions.go({ page: "session", id: host === "local" ? id : `${host}~${id}` });
    close();
  };
  return createPortal(
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="modal import-modal" role="dialog" aria-modal="true" aria-label="Import from Claude Code">
        <header className="modal-head">
          <h2>Import from Claude Code</h2>
          <span className="spacer" />
          <button className="btn-icon" aria-label="Close" onClick={close}>
            <Icon d={CLOSE} size={14} />
          </button>
        </header>
        <p className="muted small">Sessions you ran with Claude Code on {machine}. An imported session opens in its own folder and its next message continues that conversation.</p>
        <div className="row">
          {state.hosts.length > 0 && <HostPicker state={state} value={host} onChange={setHost} onAddHost={() => (actions.go({ page: "hosts" }), close())} />}
          <input
            className="import-filter"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter by title, folder or branch"
            aria-label="Filter sessions"
            spellCheck={false}
            autoFocus
          />
        </div>
        {state.importError && <p className="bad">{state.importError}</p>}
        {list?.error && <p className="bad">{list.error}</p>}
        {!list && <p className="muted">Loading Claude Code sessions…</p>}
        {list && !list.error && rows.length === 0 && (
          <p className="muted">{list.sessions.length ? "No session matches." : `No Claude Code sessions on ${machine}.`}</p>
        )}
        {rows.length > 0 && (
          <ul className="import-list">
            {rows.map((s) => (
              <li key={s.sessionId} className="import-row">
                <div className="import-text">
                  <div className="import-title">{s.title}</div>
                  <div className="muted small import-meta">
                    {/* The folder and branch give way first: the age tells sessions in one folder apart. */}
                    <span className="import-where">
                      {s.cwd ? tilde(s.cwd, homeOn(state, host)) : "unknown folder"}
                      {s.gitBranch ? ` · ${s.gitBranch}` : ""}
                    </span>
                    <span className="import-when">{` · ${ago(s.lastModified)}`}</span>
                  </div>
                  {!s.importedAs && Date.now() - s.lastModified < RECENT_MS && (
                    // Two writers on one Claude Code conversation would interleave.
                    <div className="warn small">Used in the last 10 minutes: if it is still open in Claude Code, close it there first.</div>
                  )}
                </div>
                {s.importedAs ? (
                  <button className="btn" aria-label={`Open ${s.title}`} onClick={() => open(s.importedAs!)}>
                    Open
                  </button>
                ) : (
                  <button
                    className="btn"
                    aria-label={`Import ${s.title}`}
                    disabled={pending !== null}
                    onClick={() => {
                      setPending(s.sessionId);
                      actions.importClaudeSession(host, s.sessionId);
                    }}
                  >
                    {pending === s.sessionId ? "Importing…" : "Import"}
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>,
    document.body,
  );
}

/** What the Hosts page can do with a host's accounts. */
interface AccountActions {
  login(engine: Engine, account: string): void;
  add(engine: Engine, name: string): void;
  remove(id: string): void;
}

function HostSummary({ state, id, selected, onSelect }: { state: State; id: string; selected: boolean; onSelect(): void }) {
  const status = hostState(state, id);
  const h = status === "down" || status === "connecting" ? undefined : state.host[id];
  const entry = state.hosts.find((h) => h.id === id);
  const where = entry ? (entry.kind === "ssh" ? entry.destination : entry.url) : "This machine";
  return (
    <button className="btn-quiet host-summary" aria-label={`Select ${hostName(state, id)}`} aria-pressed={selected} onClick={onSelect}>
      <span className="host-summary-identity">
        <span className="host-summary-name">
          <span className={`dot host-dot-${status}`} />
          <strong>{hostName(state, id)}</strong>
          {entry && <Cloud size={13} />}
        </span>
        <span className="muted small">{where}</span>
        <span className={`small host-state-${status}`}>{!h && status === "ok" ? "Waiting for stats…" : HOST_STATE_LABEL[status]}</span>
      </span>
      {(
        [
          ["CPU", h ? pct(h.cpu) : "—"],
          ["Memory", h ? `${gb(h.memUsed)} GB` : "—"],
          ["Disk", h ? `${gb(h.diskUsed)} GB` : "—"],
        ] as const
      ).map(([label, value]) => (
        <span key={label} className="host-summary-metric">
          <span className="muted small">{label}</span>
          <strong>{value}</strong>
        </span>
      ))}
      <Icon d={CHEVRON_RIGHT} size={13} />
    </button>
  );
}

function HostInspector({
  state,
  actions,
  id,
  onLogin,
  accountActions,
  initialTab = "sessions",
}: {
  state: State;
  actions: Actions;
  id: string;
  initialTab?: "sessions" | "engines" | "system";
  onLogin(engine: Engine): void;
  accountActions: AccountActions;
}) {
  const [tab, setTab] = useState<"sessions" | "engines" | "system">(initialTab);
  const entry = state.hosts.find((h) => h.id === id);
  const status = hostState(state, id);
  const host = status === "down" || status === "connecting" ? undefined : state.host[id];
  const sessions = Object.values(state.sessions).filter((s) => hostOf(s) === id);
  const active = sessions.filter(
    (s) => s.status === "running" || s.status === "needs_approval" || s.status === "queued" || host?.sessions?.some((u) => u.id === s.id),
  );
  const install = state.installs[id];
  const problem = entry && status === "down" ? explainHostError(entry) : null;
  const name = hostName(state, id);
  const engines = hostOnline(state, id) ? state.engines[id] : undefined;
  const count = active.length;
  return (
    <section className="host-inspector" aria-label={`${name} details`}>
      <div className="host-inspector-heading">
        <h2>{name}</h2>
        {entry && <Cloud size={13} />}
        <span className="spacer" />
        <span className={`small host-state-${status}`}>{HOST_STATE_LABEL[status]}</span>
      </div>
      {install && (install.running || !install.ok || status !== "ok") && <InstallLog install={install} machine={name} />}
      {problem && !install?.running && (
        <div className="host-problem">
          <strong>{problem.title}</strong>
          {problem.hint && <p className="muted small">{problem.hint}</p>}
          {problem.install && entry?.kind === "ssh" && (
            <button className="btn" onClick={() => actions.installHost(id)}>
              Install Switchyard on {name}
            </button>
          )}
          {entry?.error && problem.title !== entry.error && (
            <details>
              <summary>Details</summary>
              <code>{entry.error}</code>
            </details>
          )}
        </div>
      )}
      {status === "connecting" && <p className="muted">Opening the SSH tunnel…</p>}
      <div className="focused-tabs" role="tablist" aria-label="Host details">
        {(
          [
            ["sessions", `Sessions${count ? ` · ${count}` : ""}`],
            ["engines", "Engine logins"],
            ["system", "System"],
          ] as const
        ).map(([key, label]) => (
          <button
            className="tab"
            key={key}
            id={`host-tab-${key}`}
            role="tab"
            aria-selected={tab === key}
            aria-controls="host-panel"
            onClick={() => setTab(key)}
            onKeyDown={(e) => {
              const keys = ["sessions", "engines", "system"] as const;
              if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) return;
              e.preventDefault();
              const next = e.key === "Home" ? "sessions" : e.key === "End" ? "system" : keys[(keys.indexOf(key) + (e.key === "ArrowRight" ? 1 : 2)) % 3];
              setTab(next);
              document.getElementById(`host-tab-${next}`)?.focus();
            }}
            tabIndex={tab === key ? 0 : -1}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="host-inspector-panel" id="host-panel" role="tabpanel" aria-labelledby={`host-tab-${tab}`}>
        {tab === "sessions" && (
          <>
            {host ? (
              <SessionRows state={state} actions={actions} sessions={active} empty={`Nothing running on ${name}`} />
            ) : (
              <p className="muted">{hostOnline(state, id) ? "Waiting for session activity…" : "Session activity is available when this host reconnects."}</p>
            )}
            <button className="btn-text" onClick={() => actions.go({ page: "sessions" })}>
              View all sessions →
            </button>
          </>
        )}
        {tab === "engines" && (
          <>
            {engines ? (
              <Engines
                engines={engines}
                accounts={accountsOn(state, id)}
                accountActions={accountActions}
                errorCount={state.errorCount}
                connected={state.connected && hostOnline(state, id)}
                onLogin={onLogin}
              />
            ) : (
              <p className="muted">{hostOnline(state, id) ? "Checking engine logins…" : "Engine logins are available when this host reconnects."}</p>
            )}
            <button className="btn-text" onClick={() => actions.go({ page: "subscriptions" })}>
              View subscription usage →
            </button>
          </>
        )}
        {tab === "system" && (
          <>
            <dl className="host-system">
              <div>
                <dt>Connection</dt>
                <dd>{entry ? entry.destination || entry.url : "This machine"}</dd>
              </div>
              <div>
                <dt>Session capacity</dt>
                <dd>{host?.pressure || HOST_STATE_LABEL[status]}</dd>
              </div>
              {host && (
                <div>
                  <dt>Processor</dt>
                  <dd>
                    {host.cores} cores · load {host.load.toFixed(1)}
                  </dd>
                </div>
              )}
            </dl>
            {host && (
              <>
                <div className="stat-grid">
                  <Stat
                    label="CPU"
                    value={host.cpu}
                    big={pct(host.cpu)}
                    detail={`load ${host.load.toFixed(1)} of ${host.cores}`}
                    warn={host.load > host.cores}
                  />
                  <Stat
                    label="Memory"
                    value={host.memTotal ? (host.memUsed / host.memTotal) * 100 : 0}
                    big={`${gb(host.memUsed)} GB`}
                    detail={`of ${gb(host.memTotal)} GB`}
                  />
                  <Stat
                    label="Disk"
                    value={host.diskTotal ? (host.diskUsed / host.diskTotal) * 100 : 0}
                    big={`${gb(host.diskUsed)} GB`}
                    detail={`of ${gb(host.diskTotal)} GB`}
                  />
                </div>
                <Sparkline values={host.cpuHistory ?? []} />
                <QueuePolicy host={host} />
              </>
            )}
            {entry && (
              <RemoveHost
                onConfirm={() => actions.removeHost(id)}
                onUpdate={entry.kind === "ssh" && host && !install?.running ? () => actions.installHost(id) : undefined}
              />
            )}
          </>
        )}
      </div>
    </section>
  );
}

/** The installer's output while it runs, and its outcome when it failed. */
function InstallLog({ install, machine }: { install: State["installs"][string]; machine: string }) {
  const lines = install.lines.filter((l) => !l.startsWith("@@"));
  const failure = install.lines.find((l) => l.startsWith("@@fail "))?.slice(7);
  return (
    <div className="host-problem">
      <div>
        {install.running
          ? `Installing Switchyard on ${machine}…`
          : install.ok
            ? `Installed. Connecting to ${machine}…`
            : `The install on ${machine} failed${failure ? `: ${failure}` : "."}`}
      </div>
      {lines.length > 0 && (
        <pre className="install-log" aria-live="polite">
          {lines.slice(-8).join("\n")}
        </pre>
      )}
    </div>
  );
}

/** What the installer found about the engines, with what to run for each that is not ready. */
function EngineSummary({ install, machine }: { install: State["installs"][string]; machine: string }) {
  const engines = install.lines
    .filter((l) => l.startsWith("@@engine "))
    .map((l) => l.split(" "))
    .map(([, name, state]) => ({ name, state }));
  const notReady = engines.filter((e) => e.state !== "ok");
  if (!engines.length) return null;
  return (
    <div className="engines">
      {engines.map((e) => (
        <div key={e.name} className="engine-row">
          <span className={`dot ${e.state === "ok" ? "host-dot-ok" : "host-dot-busy"}`} aria-hidden="true" />
          <span>{ENGINE_LABEL[e.name as Engine] ?? e.name}</span>
          <span className="muted small">{e.state === "ok" ? "ready" : e.state === "missing" ? "not installed" : "not logged in"}</span>
          {e.state !== "ok" && <code className="engine-cmd">{e.state === "missing" ? ENGINE_INSTALL[e.name as Engine] : ENGINE_LOGIN[e.name as Engine]}</code>}
        </div>
      ))}
      {notReady.length > 0 && <div className="muted small">Run the command on {machine} once, over SSH. Sessions with that engine work after that.</div>}
    </div>
  );
}

/** Each engine on a host and whether it is ready, with a login for one that is not. */
function Engines(props: {
  engines: EngineStates;
  accounts: AccountEntry[];
  accountActions?: AccountActions;
  errorCount: number;
  connected: boolean;
  onLogin?(engine: Engine): void;
}) {
  const { engines, accounts, accountActions, errorCount, connected, onLogin } = props;
  // The engine whose Add account form is open.
  const [adding, setAdding] = useState<Engine | null>(null);
  return (
    <div className="engines">
      {ENGINES.map((engine) => {
        const st = engines[engine];
        if (!st) return null;
        const ready = st.installed && st.loggedIn === true;
        return (
          <div key={engine} className="engine-block">
            <div className="engine-row">
              <span className={`dot ${ready ? "host-dot-ok" : "host-dot-busy"}`} aria-hidden="true" />
              <span>{ENGINE_LABEL[engine]}</span>
              <span className="muted small">{stateText(st)}</span>
              {/* Either the install command or one action group sits at the right: a narrow row wraps the group whole instead of
                  leaving Add account at the left, and an empty group would share the free space with the install command. */}
              {!st.installed ? (
                <code className="engine-cmd">{ENGINE_INSTALL[engine]}</code>
              ) : (
                <span className="engine-actions">
                  {(st.loggedIn !== true || addsProviders(engine)) && onLogin && (
                    <button className="btn engine-login" onClick={() => onLogin(engine)}>
                      {addsProviders(engine) && st.loggedIn === true ? "Add provider" : "Log in"}
                    </button>
                  )}
                  {accountActions && adding !== engine && (
                    <button className="btn-text engine-add" aria-label={`Add ${ENGINE_LABEL[engine]} account`} onClick={() => setAdding(engine)}>
                      <Icon d={PLUS} size={12} />
                      Add account
                    </button>
                  )}
                </span>
              )}
            </div>
            {accounts
              .filter((a) => a.engine === engine)
              .map((a) => (
                <AccountRow
                  key={a.id}
                  account={a}
                  state={engines.accounts?.[a.id]}
                  onLogin={accountActions ? () => accountActions.login(engine, a.id) : undefined}
                  onRemove={accountActions ? () => accountActions.remove(a.id) : undefined}
                />
              ))}
            {adding === engine && accountActions && (
              <AddAccount
                engine={engine}
                accountIds={accounts.map((a) => a.id)}
                errorCount={errorCount}
                connected={connected}
                onAdd={(name) => accountActions.add(engine, name)}
                onDone={() => setAdding(null)}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

/** An added account under its engine: its login state, Log in and Remove
 *  (which asks for a second click, like removing a host). */
function AccountRow({ account, state: st, onLogin, onRemove }: { account: AccountEntry; state?: EngineState; onLogin?(): void; onRemove?(): void }) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(false), 3000);
    return () => clearTimeout(timer);
  }, [armed]);
  const ready = st?.installed && st.loggedIn === true;
  return (
    <div className="account-row">
      <span className={`dot ${ready ? "host-dot-ok" : "host-dot-busy"}`} aria-hidden="true" />
      <span className="account-name">{account.name}</span>
      <span className="muted small">{st ? stateText(st) : "checking…"}</span>
      {st?.installed && (st.loggedIn !== true || addsProviders(account.engine)) && onLogin && (
        <button className="btn engine-login" aria-label={`Log in ${account.name}`} onClick={onLogin}>
          {addsProviders(account.engine) && st.loggedIn === true ? "Add provider" : "Log in"}
        </button>
      )}
      {onRemove && (
        <button
          className="btn-text account-remove"
          aria-label={`Remove ${account.name}`}
          title="Deletes the account's login on this host"
          // Disarm on the confirming click: if yardd refuses, the row should
          // not keep offering a click that will be refused again.
          onClick={() => {
            setArmed(!armed);
            if (armed) onRemove();
          }}
        >
          {armed ? "Click again to remove" : "Remove"}
        </button>
      )}
    </div>
  );
}

function AddAccount(props: { engine: Engine; accountIds: string[]; errorCount: number; connected: boolean; onAdd(name: string): void; onDone(): void }) {
  const { accountIds, errorCount, connected, onAdd, onDone } = props;
  const [name, setName] = useState("");
  // The account ids known at the submit, until yardd answers. A refusal (a
  // name already in use) keeps the form open with the name; the new account
  // appearing closes it, and a dropped connection gives Add back. A timer used
  // to close it whatever yardd said.
  const [sent, setSent] = useState<string[] | null>(null);
  const errors = useRef(errorCount);
  useEffect(() => {
    if (sent && errorCount !== errors.current) setSent(null);
  }, [errorCount]);
  useEffect(() => setSent(null), [connected]);
  useEffect(() => {
    if (sent && accountIds.some((id) => !sent.includes(id))) onDone();
  }, [sent, accountIds]);
  return (
    <form
      className="account-add"
      onSubmit={(e) => {
        e.preventDefault();
        if (!name.trim() || sent) return;
        errors.current = errorCount;
        setSent(accountIds);
        onAdd(name.trim());
      }}
    >
      <input
        autoFocus
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder={`Name, such as Work`}
        maxLength={40}
        spellCheck={false}
        autoComplete="off"
        aria-label="Account name"
        onKeyDown={(e) => e.key === "Escape" && onDone()}
      />
      <button className="btn-primary" type="submit" disabled={!name.trim() || !!sent}>
        Add
      </button>
      <button className="btn-text" type="button" onClick={onDone}>
        Cancel
      </button>
    </form>
  );
}

function Stat({ label, value, big, detail, warn }: { label: string; value: number; big: string; detail: string; warn?: boolean }) {
  return (
    <div className={`stat ${warn ? "warn" : ""}`}>
      <span className="muted small">{label}</span>
      <span className="stat-big">{big}</span>
      <div className="bar-track" aria-hidden="true">
        <div className="bar-fill" style={{ width: `${Math.max(0, Math.min(100, value))}%` }} />
      </div>
      <span className="muted small">{detail}</span>
    </div>
  );
}

/** Removing asks for a second click, like deleting a session. */
function RemoveHost({ onConfirm, onUpdate }: { onConfirm(): void; onUpdate?(): void }) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(false), 3000);
    return () => clearTimeout(timer);
  }, [armed]);
  return (
    <div className="host-foot">
      <button className="btn-text" onClick={() => (armed ? onConfirm() : setArmed(true))}>
        {armed ? "Click again to remove" : "Remove host"}
      </button>
      {armed && <span className="muted small">Its sessions keep running there.</span>}
      {onUpdate && !armed && (
        <button className="btn-text" onClick={onUpdate} title="Install the latest Switchyard there and restart it">
          Update Switchyard
        </button>
      )}
    </div>
  );
}

function AddHost(props: { actions: Actions; hosts: HostEntry[]; errorCount: number; connected: boolean; onDone(): void }) {
  const { actions, hosts, errorCount, connected, onDone } = props;
  const [destination, setDestination] = useState("");
  const [name, setName] = useState("");
  const [port, setPort] = useState("47717");
  const [more, setMore] = useState(false);
  const [direct, setDirect] = useState(false);
  const [url, setUrl] = useState("");
  // The host ids known when Add was clicked, until yardd answers. A new host
  // appearing closes the form; an error keeps it open with what was typed, and
  // a dropped connection gives Add back. Closing on the click itself lost the
  // input whenever yardd refused.
  const [sent, setSent] = useState<string[] | null>(null);
  useEffect(() => setSent(null), [errorCount, connected]);
  useEffect(() => {
    if (sent && hosts.some((h) => !sent.includes(h.id))) onDone();
  }, [sent, hosts]);
  const finalName = (name.trim() || (direct ? "" : nameFromDestination(destination))).trim();
  // A DNS label may be up to 63 characters, so a derived name can be one yardd
  // refuses. Say so next to the Name field, opening Options to show it, rather
  // than send it and get a banner about a field the user never saw.
  const tooLong = Array.from(finalName).length > HOST_NAME_LIMIT;
  useEffect(() => {
    if (tooLong) setMore(true);
  }, [tooLong]);
  const nameNote = useId();
  const ready = finalName && !tooLong && (direct ? url.trim() : destination.trim());
  const submit = () => {
    if (!ready || sent) return;
    actions.addHost(
      direct
        ? { name: finalName, kind: "direct", url: url.trim() }
        : { name: finalName, kind: "ssh", destination: destination.trim(), remotePort: Number(port) || 47717 },
    );
    setSent(hosts.map((h) => h.id));
  };
  return (
    <section className="host-card add-host" aria-label="Add host">
      <h2>Add a host</h2>
      <div className="muted small">
        Run sessions on another machine you can <code>ssh</code> into without a password. If Switchyard isn't there yet, you can install it from here next.
      </div>
      {!direct && (
        <input
          className="field"
          autoFocus
          type="text"
          placeholder="user@machine"
          aria-label="SSH destination"
          value={destination}
          onChange={(e) => setDestination(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
          spellCheck={false}
        />
      )}
      {more && (
        <div className="form-grid">
          <label htmlFor="host-name">Name</label>
          <input
            id="host-name"
            type="text"
            placeholder={nameFromDestination(destination) || "devbox"}
            value={name}
            onChange={(e) => setName(e.target.value)}
            aria-invalid={tooLong || undefined}
            aria-describedby={tooLong ? nameNote : undefined}
            spellCheck={false}
          />
          {tooLong && (
            <>
              <span />
              <span id={nameNote} className="bad small">
                {name.trim() ? "" : "The name from the destination is too long. "}Use {HOST_NAME_LIMIT} characters or fewer.
              </span>
            </>
          )}
          {direct ? (
            <>
              <label htmlFor="host-url">URL</label>
              <input id="host-url" type="text" placeholder="ws://127.0.0.1:47726/ws" value={url} onChange={(e) => setUrl(e.target.value)} spellCheck={false} />
            </>
          ) : (
            <>
              <label htmlFor="host-port">Remote port</label>
              <input id="host-port" type="text" inputMode="numeric" value={port} onChange={(e) => setPort(e.target.value.replace(/\D/g, ""))} />
            </>
          )}
          <span />
          <label className="check small">
            <input type="checkbox" checked={direct} onChange={(e) => setDirect(e.target.checked)} />
            Direct WebSocket URL, for testing
          </label>
        </div>
      )}
      <div className="row add-host-actions">
        <button className="btn-primary" disabled={!ready || !!sent} onClick={submit}>
          Add {finalName || "host"}
        </button>
        <button className="btn-text" onClick={onDone}>
          Cancel
        </button>
        <span className="spacer" />
        {!more && (
          <button className="btn-text" onClick={() => setMore(true)}>
            Options
          </button>
        )}
      </div>
    </section>
  );
}

function Hosts({ state, actions }: { state: State; actions: Actions }) {
  const [adding, setAdding] = useState(false);
  const route = state.route.page === "hosts" ? state.route : null;
  const [selected, setSelected] = useState(route?.host ?? "local");
  const id = selected === "local" || state.hosts.some((h) => h.id === selected) ? selected : "local";
  const [login, setLogin] = useState<{ host: string; engine: Engine; account?: string } | null>(null);
  const page = useRef<HTMLDivElement>(null);
  // Bumped by a tap on a host row, so tapping the selected host again also reveals it.
  const [reveal, setReveal] = useState(0);
  useEffect(() => {
    if (!reveal) return;
    // On a phone the setup card and the host rows push the details below the
    // fold, so a tap on a host only moved its border. Already in view: stay put.
    const heading = page.current?.querySelector<HTMLElement>(".host-inspector-heading");
    if (heading && heading.getBoundingClientRect().bottom > window.innerHeight) heading.scrollIntoView({ block: "start" });
  }, [reveal]);
  // Sign in in the usage popover and the sidebar's host rows navigate here
  // while Hosts may already be open; the state above only read the first
  // route, so the URL changed and the page did not. `go` makes a new route
  // object on every navigation, so this runs once per arrival.
  const [arrival, setArrival] = useState(0);
  const firstRoute = useRef(route);
  useEffect(() => {
    if (!route || route === firstRoute.current) return;
    if (route.host) setSelected(route.host);
    if (route.host || route.tab) {
      setArrival((n) => n + 1);
      setReveal((n) => n + 1);
    }
  }, [route]);
  const accountActions: AccountActions = {
    login: (engine, account) => setLogin({ host: id, engine, account }),
    add: (engine, name) => actions.addAccount(id, engine, name),
    remove: (account) => actions.removeAccount(id, account),
  };
  return (
    <div className="hosts">
      <header className="bar">
        <MenuButton />
        <h1>Hosts</h1>
        <span className="spacer" />
        {!adding && (
          <button className="btn" onClick={() => setAdding(true)}>
            <Icon d={PLUS} size={13} />
            Add host
          </button>
        )}
      </header>
      <div className="scroll">
        <div className="focused-page" ref={page}>
          {/* The session headers' phone shortcut leads here: keep setup first. */}
          <MobileSetup
            mobile={state.mobile}
            targets={phoneTargets(
              [{ id: "local", name: hostName(state, "local"), mobile: state.mobile }, ...state.hosts.map((h) => ({ id: h.id, name: h.name, mobile: state.hostMobile[h.id] }))],
              location.origin,
            )}
          />
          {adding && (
            <AddHost actions={actions} hosts={state.hosts} errorCount={state.errorCount} connected={state.connected} onDone={() => setAdding(false)} />
          )}
          <div className="focused-hosts" aria-label="Choose a host">
            {["local", ...state.hosts.map((h) => h.id)].map((h) => (
              <HostSummary
                key={h}
                state={state}
                id={h}
                selected={h === id}
                onSelect={() => {
                  setSelected(h);
                  setReveal((n) => n + 1);
                }}
              />
            ))}
          </div>
          <HostInspector
            key={`${id}:${arrival}`}
            initialTab={route?.host === undefined || route.host === id ? route?.tab : undefined}
            state={state}
            actions={actions}
            id={id}
            onLogin={(engine) => setLogin({ host: id, engine })}
            accountActions={accountActions}
          />
          {state.hosts.length === 0 && !adding && (
            <button className="btn-quiet host-empty" onClick={() => setAdding(true)}>
              <Icon d={CLOUD} />
              <span>
                <strong>Run sessions on another machine</strong>
                <span className="muted small">Add a server or desktop you reach over SSH.</span>
              </span>
            </button>
          )}
        </div>
      </div>
      {login && <LoginDialog state={state} actions={actions} host={login.host} engine={login.engine} account={login.account} onClose={() => setLogin(null)} />}
    </div>
  );
}

function SubscriptionsPage({ state, actions }: { state: State; actions: Actions }) {
  return (
    <div className="hosts">
      <header className="bar">
        <MenuButton />
        <h1>Subscriptions</h1>
      </header>
      <div className="scroll">
        <div className="focused-page">
          <p className="muted">Your accounts and usage, across all hosts</p>
          <Subscriptions
            sources={["local", ...state.hosts.map((h) => h.id)].map((host) => ({
              host,
              name: hostName(state, host),
              usage: usageOn(state, host),
              accounts: accountsOn(state, host),
              engines: state.engines[host],
            }))}
            onManage={(host) => actions.go({ page: "hosts", ...(host ? { host } : {}), tab: "engines" })}
          />
        </div>
      </div>
    </div>
  );
}

function SessionRows({
  state,
  actions,
  sessions,
  empty = "No sessions to show",
  showHost = false,
  search,
}: {
  state: State;
  actions: Actions;
  sessions: SessionMeta[];
  empty?: string;
  showHost?: boolean;
  search?: { query: string; clear: () => void };
}) {
  // An empty search result is not an empty list: "start a session" would
  // read as if the user had none.
  if (!sessions.length && search?.query.trim())
    return (
      <div className="focused-empty" role="status">
        <strong>No sessions match your search.</strong>
        <button className="btn" onClick={search.clear}>
          Clear search
        </button>
      </div>
    );
  if (!sessions.length)
    return (
      <div className="focused-empty">
        <strong>{empty}</strong>
        <p className="muted">Start a session whenever you're ready.</p>
        <button className="btn" onClick={() => actions.go({ page: "new" })}>
          New session
        </button>
      </div>
    );
  return (
    <div className="focused-session-list">
      {sessions.map((s) => {
        const host = hostOf(s);
        const usage = state.host[host]?.sessions?.find((u) => u.id === s.id);
        const queued = state.host[host]?.queued?.find((q) => q.id === s.id)?.reason;
        return (
          <button key={s.id} className="btn-quiet focused-session-row" onClick={() => actions.go({ page: "session", id: s.id })}>
            <span className="focused-session-identity">
              <strong>{s.title}</strong>
              <span className="muted small">
                {ENGINE_NAME[s.engine]} · {accountName(accountsOn(state, host), s.engine, accountOf(s))}
                {showHost ? ` · ${hostName(state, host)}` : ""}
              </span>
              {s.status === "queued" && queued && <span className="muted small">{queued}</span>}
            </span>
            <span className={`focused-session-state ${s.status === "needs_approval" ? "warn" : "muted"}`}>
              <span className={`dot dot-${s.status}`} />
              {/* Archived sessions still run: past idle, the status is what the row is for. */}
              {!hostOnline(state, host) ? "Offline" : !s.archived ? STATUS_LABEL[s.status] : s.status === "idle" ? "Archived" : `Archived · ${STATUS_LABEL[s.status]}`}
            </span>
            <span className="focused-session-resources muted small">
              {usage ? (
                <>
                  {size(usage.memBytes)}
                  <br />
                  {usage.cpuCores.toFixed(1)} cores
                </>
              ) : (
                "—"
              )}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function SessionsPage({ state, actions }: { state: State; actions: Actions }) {
  const [filter, setFilter] = useState<"active" | "all" | "archived">("active");
  const [query, setQuery] = useState("");
  const sessions = Object.values(state.sessions)
    .filter((s) => !hiddenFromSidebar(s))
    .filter((s) => (filter === "archived" ? s.archived : filter === "all" || !s.archived))
    // Search what the row shows, account included: "Work" should find the
    // sessions that run on the Work login.
    .filter((s) =>
      `${s.title} ${s.cwd} ${ENGINE_NAME[s.engine]} ${accountName(accountsOn(state, hostOf(s)), s.engine, accountOf(s))} ${hostName(state, hostOf(s))}`
        .toLowerCase()
        .includes(query.toLowerCase()),
    )
    .sort((a, b) => b.updatedAt - a.updatedAt);
  return (
    <div className="hosts">
      <header className="bar">
        <MenuButton />
        <h1>Sessions</h1>
        <span className="spacer" />
        <button className="btn" onClick={() => actions.go({ page: "new" })}>
          <Icon d={PLUS} />
          New session
        </button>
      </header>
      <div className="scroll">
        <div className="focused-page">
          <div className="sessions-toolbar">
            <div className="focused-tabs" role="group" aria-label="Session visibility">
              {(
                [
                  ["active", "Active"],
                  ["all", "All sessions"],
                  ["archived", "Archived"],
                ] as const
              ).map(([key, label]) => (
                <button className={`tab ${filter === key ? "active" : ""}`} key={key} aria-pressed={filter === key} onClick={() => setFilter(key)}>
                  {label}
                </button>
              ))}
            </div>
            <input className="field" aria-label="Search sessions" placeholder="Search sessions" value={query} onChange={(e) => setQuery(e.target.value)} />
          </div>
          <SessionRows state={state} actions={actions} sessions={sessions} showHost search={{ query, clear: () => setQuery("") }} />
        </div>
      </div>
    </div>
  );
}
