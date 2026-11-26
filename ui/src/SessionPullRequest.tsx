import { useEffect } from "react";
import { prLink } from "./prPeek";
import type { PrResult } from "./protocol";

export function SessionPullRequest({ sessionId, online, busy, pr, load }: {
  sessionId: string;
  online: boolean;
  busy: boolean;
  pr?: PrResult;
  load(id: string): void;
}) {
  useEffect(() => {
    if (!online || busy) return;
    load(sessionId);
    const timer = setInterval(() => load(sessionId), 60_000);
    return () => clearInterval(timer);
  }, [sessionId, online, busy, load]);
  if (!pr || !("number" in pr) || !prLink(pr.url)) return null;
  return <a className="session-pr-link" href={pr.url} target="_blank" rel="noopener noreferrer" aria-label={`Pull request #${pr.number}`} title={pr.title}>PR #{pr.number}</a>;
}
