import { useState, type RefObject } from "react";
import { accountChoices, accountOf, DEFAULT_ACCOUNT } from "./accounts";
import { AnchoredPopover } from "./AnchoredPopover";
import { ENGINES, ENGINE_NAME } from "./engines";
import type { Engine, SessionMeta } from "./protocol";
import { SelectPicker } from "./SelectPicker";
import { accountsOn, hostOf, modelsOn, type Actions, type State } from "./store";

export function SessionHandover({ anchor, session, state, actions, onClose }: {
  anchor: RefObject<HTMLButtonElement | null>;
  session: SessionMeta;
  state: State;
  actions: Actions;
  onClose(): void;
}) {
  const host = hostOf(session);
  const catalog = modelsOn(state, host);
  const [engine, setEngine] = useState<Engine>(session.engine);
  const [account, setAccount] = useState(accountOf(session));
  const [model, setModel] = useState(session.model);
  const models = catalog[engine] ?? [];
  const accounts = accountChoices(accountsOn(state, host), engine);
  const selectedModel = models.find((entry) => entry.id === model);
  const busy = ["running", "needs_approval", "queued"].includes(session.status);
  const changed = engine !== session.engine || account !== accountOf(session) || model !== session.model;
  const valid = !!selectedModel && accounts.some((entry) => entry.id === account);
  const chooseEngine = (next: Engine) => {
    if (next === engine) return;
    const available = catalog[next] ?? [];
    setEngine(next);
    setAccount(next === session.engine ? accountOf(session) : DEFAULT_ACCOUNT);
    setModel(next === session.engine ? session.model : (available.find((entry) => entry.isDefault) ?? available[0])?.id ?? "");
  };
  return (
    <AnchoredPopover anchor={anchor} label="Handover session" onClose={onClose}>
      <div className="popover-heading"><strong>Handover session</strong></div>
      <p className="muted">Continue in the same folder. The new session will appear under this one.</p>
      <div className="handover-fields">
        <span className="muted">Agent</span>
        <SelectPicker label="Agent" value={engine} onChange={chooseEngine} options={ENGINES.filter((id) => catalog[id]?.length).map((value) => ({ value, label: ENGINE_NAME[value] }))} />
        <span className="muted">Account</span>
        <SelectPicker label="Account" value={account} onChange={setAccount} options={accounts.map((entry) => ({ value: entry.id, label: entry.name }))} />
        <span className="muted">Model</span>
        <SelectPicker label="Model" value={model} onChange={setModel} options={models.map((entry) => ({ value: entry.id, label: entry.name }))} />
      </div>
      {busy && <p className="muted">Stop the current turn or wait for it to finish before handing over.</p>}
      <div className="popover-footer">
        <button className="btn-text" onClick={onClose}>Cancel</button>
        <button className="btn-primary" disabled={busy || !changed || !valid} onClick={() => {
          if (!selectedModel) return;
          const effort = selectedModel.efforts.includes(session.effort) ? session.effort : selectedModel.defaultEffort ?? selectedModel.efforts[0] ?? "";
          actions.handoff(session.id, engine, model, effort, account);
          onClose();
        }}>Handover</button>
      </div>
    </AnchoredPopover>
  );
}
