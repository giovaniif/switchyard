import { useEffect, useState } from "react";

// The Mac app tells its page about a release it found (app/src-tauri
// main.rs): it sets window.__SWITCHYARD_UPDATE__ and fires `switchyard:update`.
// `ready`: installed over the app, a restart runs it; otherwise only found.
// A plain browser sets neither, so it never shows the notice.
export type AppUpdate = { version: string; ready: boolean };

export function readAppUpdate(value: unknown): AppUpdate | null {
  if (!value || typeof value !== "object") return null;
  const { version, ready } = value as { version?: unknown; ready?: unknown };
  if (typeof version !== "string" || version.length > 40 || !/^\d+\.\d+\.\d+/.test(version)) return null;
  return { version, ready: ready === true };
}

/** Found and then installed are two things to say, so each is dismissed on its own. */
export function updateKey(update: AppUpdate): string {
  return `${update.version}${update.ready ? ":ready" : ""}`;
}

// "notes": the app's window with the release notes and Install and Restart.
// "restart": the app asks before it restarts into an installed release.
export type UpdateAction = "notes" | "restart";

/** The app hears this as a Tauri event; false where there is no app. */
export function sendUpdateAction(action: UpdateAction, w: any = window): boolean {
  const emit = w.__TAURI__?.event?.emit;
  if (typeof emit !== "function") return false;
  Promise.resolve(emit("switchyard:update-action", action)).catch(() => {});
  return true;
}

const DISMISSED = "yard.updateDismissed";

export function useAppUpdate(): { update: AppUpdate | null; dismiss: () => void } {
  const [update, setUpdate] = useState(() => readAppUpdate((window as any).__SWITCHYARD_UPDATE__));
  const [dismissed, setDismissed] = useState(() => {
    try { return localStorage.getItem(DISMISSED); } catch { return null; }
  });
  useEffect(() => {
    const heard = (e: Event) => setUpdate(readAppUpdate((e as CustomEvent).detail));
    window.addEventListener("switchyard:update", heard);
    // The app may have spoken between the first render and this listener.
    setUpdate(readAppUpdate((window as any).__SWITCHYARD_UPDATE__));
    return () => window.removeEventListener("switchyard:update", heard);
  }, []);
  const dismiss = () => {
    if (!update) return;
    const key = updateKey(update);
    try { localStorage.setItem(DISMISSED, key); } catch {}
    setDismissed(key);
  };
  return { update: update && updateKey(update) !== dismissed ? update : null, dismiss };
}
