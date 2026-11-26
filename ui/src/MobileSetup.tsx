import { useEffect, useMemo, useState } from "react";
import { encode } from "uqr";
import { Icon } from "./App";
import { inMobileApp, mobileHost, mobileLink, phoneState, testFlightLink, type MobileStatus, type PhoneTarget } from "./mobile";

const PHONE = "M8 2h8a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2ZM11 18h2";
const COPY = "M9 9h11v11H9zM5 15H4V4h11v1";
const CHECK = "M5 12l5 5 9-11";

/** The address typed by hand, for hosts that list none a phone can open. */
const TYPED = "yard.mobileHost";

/** Shown in headers until a phone has connected to any of the hosts. */
export function MobileSetupButton({ mobile, hostMobile, open }: { mobile: MobileStatus; hostMobile: Record<string, MobileStatus>; open(): void }) {
  if (mobile.configured || Object.values(hostMobile).some((m) => m?.configured) || inMobileApp()) return null;
  return <button className="btn-icon mobile-setup-button" aria-label="Set up iPhone app" title="Set up iPhone app" onClick={open}><Icon d={PHONE} size={16} /></button>;
}

/** A QR code drawn as one SVG path: the iPhone's camera opens the link in the app. */
function QrCode({ text, label }: { text: string; label: string }) {
  const { size, path } = useMemo(() => {
    const qr = encode(text, { ecc: "M", border: 2 });
    let d = "";
    qr.data.forEach((row, y) => row.forEach((dark, x) => { if (dark) d += `M${x} ${y}h1v1h-1z`; }));
    return { size: qr.size, path: d };
  }, [text]);
  return (
    <svg className="phone-qr-code" viewBox={`0 0 ${size} ${size}`} role="img" aria-label={label} shapeRendering="crispEdges">
      <rect width={size} height={size} />
      <path d={path} />
    </svg>
  );
}

export function MobileSetup({ mobile, targets }: { mobile: MobileStatus; targets: PhoneTarget[] }) {
  const [typed, setTyped] = useState(() => localStorage.getItem(TYPED) ?? "");
  const [choice, setChoice] = useState(0);
  const [editing, setEditing] = useState(false);
  const [copied, setCopied] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(t);
  }, []);

  const typedHost = mobileHost(typed);
  const picked = targets[Math.min(choice, targets.length - 1)];
  const custom = editing || !picked;
  const origin = custom ? typedHost : picked.origin;
  const status = custom ? mobile : picked.status;
  const link = origin ? mobileLink(origin) : null;
  const state = phoneState(status, now);
  const invite = testFlightLink(mobile.testFlightUrl ?? picked?.status.testFlightUrl ?? undefined);
  // On a phone the camera cannot scan its own screen: offer the link instead.
  const touch = typeof matchMedia === "function" && matchMedia("(pointer: coarse)").matches;
  const where = origin ? new URL(origin).host : "";

  const type = (value: string) => {
    setTyped(value);
    if (mobileHost(value)) localStorage.setItem(TYPED, value.trim());
  };
  const copy = async () => {
    if (!origin) return;
    try {
      await navigator.clipboard.writeText(origin);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // No clipboard here; the address is on screen to copy by hand.
    }
  };

  return (
    <section className="host-card mobile-setup" aria-label="iPhone app">
      <div className="mobile-setup-head">
        <Icon d={PHONE} size={18} />
        <h2>Switchyard for iPhone</h2>
        <span className="spacer" />
        <span className={`phone-state phone-state-${state.kind}`} role="status">
          <span className="dot" />
          {state.text}
        </span>
      </div>
      <div className="mobile-setup-body">
        {link && !touch && (
          <figure className="phone-qr">
            <QrCode text={link} label={`QR code that opens Switchyard on your iPhone with ${where}`} />
            <figcaption className="muted small">Scan with the iPhone Camera</figcaption>
          </figure>
        )}
        <div className="mobile-setup-steps">
          <ol className="login-steps">
            <li>
              {invite ? (
                <a className="btn-primary login-open" href={invite} target="_blank" rel="noreferrer">Install with TestFlight</a>
              ) : (
                <><strong>Install the app from TestFlight.</strong> <span className="muted">Accept the invitation on your iPhone first.</span></>
              )}
            </li>
            <li>Turn on Tailscale (or your private network) on the iPhone.</li>
            <li>
              {link ? (
                touch ? <>Open the app with this host: <a className="btn login-open mobile-open" href={link}>Open in Switchyard</a></> : <>Point the Camera at the code. Switchyard opens with <strong>{where}</strong> filled in; tap Connect.</>
              ) : (
                <>Enter the HTTPS address you open this host at on your phone.</>
              )}
            </li>
          </ol>

          {!custom && targets.length > 1 && (
            <div className="segmented phone-hosts" role="group" aria-label="Host for the phone">
              {targets.map((t, i) => (
                <button key={t.origin} aria-pressed={i === choice} onClick={() => setChoice(i)}>{t.hostName}</button>
              ))}
            </div>
          )}

          {custom ? (
            <div className="phone-address-edit">
              <label className="small muted" htmlFor="mobile-host">Address the phone can reach</label>
              <input id="mobile-host" className="field" type="url" inputMode="url" autoCapitalize="none" autoCorrect="off" spellCheck={false} placeholder="https://your-host.ts.net" value={typed} onChange={(e) => type(e.target.value)} />
              {typed && !typedHost && <p className="warn small">Use an HTTPS address your phone can open, without a path or login. Localhost and 127.0.0.1 point to the phone itself.</p>}
              {!picked && <p className="muted small">None of your hosts lists one yet. It is the address in the host's <code>YARD_ALLOW_ORIGIN</code>, such as its Tailscale name.</p>}
              {picked && <button className="btn-text" onClick={() => setEditing(false)}>Use {picked.hostName}’s address</button>}
            </div>
          ) : (
            <div className="phone-address">
              <code title={origin ?? ""}>{origin}</code>
              <button className="btn-text phone-copy" onClick={copy} aria-label="Copy address">
                <Icon d={copied ? CHECK : COPY} size={13} />
                {copied ? "Copied" : "Copy"}
              </button>
              <span className="spacer" />
              <button className="btn-text" onClick={() => { setEditing(true); if (!typed && origin) setTyped(origin); }}>Use another address</button>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
