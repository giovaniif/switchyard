// verify/yv: the one way agents drive Switchyard to prove a change works.
// Every command prints a single JSON object, even on error.
// Exit codes: 0 ok, 1 failed, 2 usage.

import { parseArgs } from "./common";
import { Fail, print, Usage, type Result } from "./out";

const HELP = `verify/yv <command> [args]   (every command prints one JSON object)

  doctor                         health, tools, build freshness, load, checkout state
  build [--force]                build yardd and ui/dist from this checkout, stamp what was built

  iso up [--name n] [--engines fake|real] [--logged-out] [--pressure] [--env YARD_X=v,...] [--no-build]
  iso seed [--name n] [--profile basic|empty]
  iso down [--name n | --all] [--keep]
  iso restart [--name n]         same port and data, like a user restarting yardd
  iso ls

  api '<op json>' [--iso n] [--wait '<match json>'] [--expect '<match json>'] [--save k=path,...] [--timeout ms] [--all]
  state [--iso n] [--save k=path] [--expect '<match json>'] [--full]
        match json: a partial object; arrays match when each wanted element matches some element;
        a string starting with ~ matches by substring
  var set|get|ls [name] [value] [--iso n]

  shot <route> [--viewport mobile|desktop|both] [--iso n] [--out dir] [--name label]
  drive <route> --steps '<json>' [--viewport ...] [--iso n] [--video] [--show-scrollbars]
        routes: new | hosts | subscriptions | sessions | settings | connectors | agents | automations | session:<id or title text> | session:{{var}}
        steps: goto click hover tap fill select type press drag wait expect upload shot aria eval reload

  map status [area]
  map run <area|all> [--recipe label] [--iso n]

  evidence init <dir> --claim "<problem>" [--head sha]
  evidence add <dir> --ref base|head --kind repro|fix|test|design|regression|note [--file f]... [--text t] [--viewport v] [--cmd "..."] [--exit n]
  evidence verify <dir>
  evidence show|markdown <dir>
  evidence retarget <dir> --head <sha>   after a push: keeps base proofs, drops head proofs
  evidence publish <dir> [--label l]   push bundle + artifacts to the verify-assets branch
  evidence fetch <bundle-url> <dir>    the verifier's copy of a published bundle

  design-check [--base ref]      design-system lint on added ui lines
  annotate --before a.png --after b.png [--box x,y,w,h] [--caption t] [--out f]
  assets push <file>... [--label pr-123] | assets check [--base ref]

  pr status|threads|comment|rerequest <pr> | pr reply <pr> <thread-id> --body t [--resolve]
  land check <pr> | land verdict <pr> --verdict PASS|PASS+NOTES|FAIL|HUMAN [--bundle dir] [--notes t] [--dry-run]
  land merge|watch <pr> [--dry-run] | land ledger [pr] | land review [--days 7]
  notify "<message>" [--title t] [--click url]   an OS notification in every open Switchyard window
  loop run patrol|maintain|review|verify|babysit [--pr N] [--engine e --model m --effort x] [--dry-run] [--force] [--iso n] [--checkout dir]
                                 verify|babysit: a run of the manual automation "Loop <kind>" with that PR's prompt
  loop automations [--apply] [--iso n] [--checkout dir]   every loop as a yardd automation (live yardd unless --iso)
  loop status | loop log <sessionId> [--last n] | loop tidy | loop pause | loop resume
  selftest [--smoke]
`;

const BOOLS = ["force", "all", "keep", "logged-out", "pressure", "no-build", "full", "video", "show-scrollbars", "dry-run", "resolve", "smoke", "allow-spend", "help", "json", "fix-hint", "keep-open", "allow-errors", "no-fetch", "no-checkout", "apply"];

async function main(argv: string[]): Promise<[string, Result]> {
  const [cmd, ...rest] = argv;
  const a = parseArgs(rest, BOOLS);
  if (!cmd || cmd === "help" || a.flags.help) return ["help", { ok: true, usage: HELP }];
  const sub = a.pos[0];
  const label = ["iso", "map", "evidence", "assets", "pr", "land", "loop", "var"].includes(cmd) && sub ? `${cmd} ${sub}` : cmd;
  const shift = () => ({ ...a, pos: a.pos.slice(1) });
  switch (cmd) {
    case "doctor":
      return [label, await (await import("./doctor")).doctor(a)];
    case "build": {
      const { build } = await import("./iso");
      return [label, build(!!a.flags.force)];
    }
    case "iso": {
      const iso = await import("./iso");
      if (sub === "up") return [label, await iso.isoUp(shift())];
      if (sub === "down") return [label, await iso.isoDown(shift())];
      if (sub === "ls") return [label, iso.isoLs()];
      if (sub === "seed") return [label, await iso.isoSeed(shift())];
      if (sub === "restart") return [label, await iso.isoRestart(shift())];
      throw new Usage("iso up|down|ls|seed|restart");
    }
    case "api":
      return [label, await (await import("./api")).api(a)];
    case "state":
      return [label, await (await import("./api")).state(a)];
    case "var":
      return [label, (await import("./api")).vars(a)];
    case "shot":
      return [label, await (await import("./browser")).shot(a)];
    case "drive":
      return [label, await (await import("./browser")).drive(a)];
    case "map":
      return [label, await (await import("./map")).map(a)];
    case "evidence":
      return [label, await (await import("./evidence")).evidence(a)];
    case "design-check":
      return [label, (await import("./design")).designCheck(a)];
    case "annotate":
      return [label, await (await import("./annotate")).annotate(a)];
    case "assets":
      return [label, (await import("./assets")).assets(a)];
    case "pr":
      return [label, await (await import("./gh")).pr(a)];
    case "land":
      return [label, await (await import("./land")).land(a)];
    case "notify":
      return [label, await (await import("./notify")).notifyCmd(a)];
    case "loop":
      return [label, await (await import("./loop")).loop(a)];
    case "selftest":
      return [label, (await import("./selftest")).selftest(a)];
    default:
      throw new Usage(`unknown command ${cmd}`);
  }
}

const argv = process.argv.slice(2);
let code: number;
try {
  const [label, result] = await main(argv);
  code = print(result, label);
} catch (e: any) {
  const label = argv.slice(0, 2).join(" ");
  if (e instanceof Usage) {
    print({ ok: false, problems: [{ code: "usage", msg: e.message, hint: "verify/yv help" }] }, label);
    code = 2;
  } else if (e instanceof Fail) {
    code = print({ ok: false, ...e.extra, problems: e.problems }, label);
  } else {
    code = print({ ok: false, problems: [{ code: "crash", msg: String(e?.stack ?? e) }] }, label);
  }
}
process.exit(code);
