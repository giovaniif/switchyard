import { TESTFLIGHT_APP } from "./mobile";

/** The page the setup QR code opens in Safari: the way into the app, or into
 *  TestFlight when the app is not installed yet. It opens no socket. */
export function PhoneWelcome({ app, invite, browser }: { app: string | null; invite: string | null; browser: string }) {
  return (
    <main className="phone-welcome">
      <section className="host-card" aria-label="Switchyard for iPhone">
        <img className="phone-welcome-icon" src="/favicon.svg" alt="" width={56} height={56} />
        <h1>Switchyard for iPhone</h1>
        {app ? (
          <>
            <a className="btn-primary phone-welcome-open" href={app}>Open in Switchyard</a>
            <p className="muted small">It opens with <strong>{new URL(browser).host}</strong> filled in; tap Connect.</p>
          </>
        ) : (
          <p className="warn small">This page is not on an HTTPS address, so the app cannot connect to it. Open the host at its HTTPS address instead.</p>
        )}
        <h2>Not installed yet?</h2>
        {invite ? (
          <a className="btn phone-welcome-open" href={invite}>Install with TestFlight</a>
        ) : (
          <ol className="login-steps">
            <li><a href={TESTFLIGHT_APP}>Get TestFlight</a> from the App Store.</li>
            <li>Open the Switchyard invitation Apple emailed you and tap <strong>View in TestFlight</strong>, then Install.</li>
            <li>Come back to this page and tap Open in Switchyard.</li>
          </ol>
        )}
        <a className="btn-text phone-welcome-browser" href={browser}>Use Switchyard in Safari instead</a>
      </section>
    </main>
  );
}
