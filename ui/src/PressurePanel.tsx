// What the owner sees of overload episodes (yardd's pressure.rs): a banner
// while a host is overloaded, the note under a turn that stalled meanwhile,
// and the history on the host's System tab. Words come from pressure.ts.
import type { Host, PressureEpisode } from "./protocol";
import { ENGINE_NAME } from "./engines";
import { bannerText, culprit, episodeLength, episodePeaks, episodeStart, mergePages, stallNote, useText } from "./pressure";
import { hostName, hostOnline, hostOf, type Actions, type State } from "./store";

/** One line per host that is overloaded now, any host: the one the owner is
 *  looking at may not be the one in trouble. Nothing when all are calm. */
export function PressureBanners({ state, actions, place }: { state: State; actions: Actions; place: "side" | "main" }) {
  const ids = ["local", ...state.hosts.map((h) => h.id)].filter((id) => (id === "local" || hostOnline(state, id)) && state.host[id]?.episode);
  if (ids.length === 0) return null;
  return (
    <>
      {ids.map((id) => (
        <PressureBanner key={id} state={state} actions={actions} id={id} place={place} />
      ))}
    </>
  );
}

function PressureBanner({ state, actions, id, place }: { state: State; actions: Actions; id: string; place: "side" | "main" }) {
  const host = state.host[id];
  const ep = host.episode!;
  const name = hostName(state, id);
  const worst = culprit(ep);
  // The worst session as this window knows it: Open always, Stop while its
  // turn runs (the session's own Stop; nothing here kills a process).
  const session = worst?.kind === "session" ? state.sessions[worst.session.id] : undefined;
  const busy = session && (session.status === "running" || session.status === "needs_approval") && hostOnline(state, hostOf(session));
  return (
    <div className={`pressure-banner pressure-${place}`} role="status" aria-label={`${name} is overloaded`}>
      <span className="pressure-text">{bannerText(name, host)}</span>
      <span className="pressure-actions">
        {session && (
          <button className="btn-text" aria-label={`Open ${session.title}`} onClick={() => actions.go({ page: "session", id: session.id })}>
            Open
          </button>
        )}
        {session && busy && (
          <button className="btn-text" aria-label={`Stop ${session.title}`} onClick={() => actions.interrupt(session.id)}>
            Stop
          </button>
        )}
        <button className="btn-text" aria-label={`${name} pressure details`} onClick={() => actions.go({ page: "hosts", host: id, tab: "system" })}>
          Details
        </button>
      </span>
    </div>
  );
}

/** Under a running turn that has said nothing for 5 minutes while its host
 *  is overloaded. Gone once yardd stops listing it (events resumed). */
export function StallNote({ state, actions, sessionId, host }: { state: State; actions: Actions; sessionId: string; host: string }) {
  const text = stallNote(state.host[host], sessionId);
  if (!text) return null;
  return (
    <div className="note stall-note" role="status">
      {text}
      <button className="btn-text note-link" onClick={() => actions.go({ page: "hosts", host, tab: "system" })}>
        See Hosts
      </button>
    </div>
  );
}

/** The System tab's list: the episode going on now, then the closed ones. */
export function PressureHistory({ state, actions, id, host }: { state: State; actions: Actions; id: string; host: Host }) {
  const older = state.pressureOlder[id];
  const closed = mergePages(host.episodes ?? [], older?.episodes ?? []);
  const all = host.episode ? [host.episode, ...closed] : closed;
  // The host message carries the latest five; a full five may have more behind.
  const more = older ? older.more : (host.episodes?.length ?? 0) >= 5;
  return (
    <section className="pressure-history" aria-label="Pressure">
      <h3>Pressure</h3>
      {all.length === 0 ? (
        <p className="muted small">No overload in the last 7 days.</p>
      ) : (
        <ol className="pressure-list">
          {all.map((ep) => (
            <EpisodeRow key={ep.id} state={state} actions={actions} ep={ep} />
          ))}
        </ol>
      )}
      {more && closed.length > 0 && (
        <button className="btn-text" disabled={older?.loading} onClick={() => actions.loadPressure(id, closed[closed.length - 1].start)}>
          {older?.loading ? "Loading…" : "Show older"}
        </button>
      )}
    </section>
  );
}

function EpisodeRow({ state, actions, ep }: { state: State; actions: Actions; ep: PressureEpisode }) {
  const open = ep.end == null;
  return (
    <li className="pressure-episode">
      <div className="pressure-episode-head">
        <strong>{episodeStart(ep)}</strong>
        <span className={open ? "small pressure-now" : "muted small"}>{open ? `Now · ${episodeLength(ep, ep.lastSeen)}` : episodeLength(ep, ep.lastSeen)}</span>
      </div>
      <div className="muted small">{episodePeaks(ep)}</div>
      {(ep.top.sessions.length > 0 || ep.top.procs.length > 0) && (
        <ul className="pressure-tops" aria-label="Top use at the peak">
          {ep.top.sessions.map((s) => (
            <li key={s.id}>
              {state.sessions[s.id] ? (
                <button className="btn-text note-link pressure-session" onClick={() => actions.go({ page: "session", id: s.id })}>
                  {s.title}
                </button>
              ) : (
                <span>{s.title}</span>
              )}
              <span className="muted small">
                {ENGINE_NAME[s.engine] ?? s.engine}
                {s.automation ? ` · ${s.automation}` : ""} · {useText(s)}
              </span>
            </li>
          ))}
          {ep.top.procs.map((p) => (
            <li key={p.pid}>
              <code title={p.cmd}>{p.cmd}</code>
              <span className="muted small">
                not a session · pid {p.pid} · {useText(p)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}
