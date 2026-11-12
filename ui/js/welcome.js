// First-run setup: install the privileged helper and the per-user polkit rule
// once, so later connects start the tunnel without a password prompt.

import { invoke } from "./api.js";
import { appendLog } from "./log.js";

const root = document.querySelector("#welcome");
const setup = document.querySelector("#welcome-setup");
const later = document.querySelector("#welcome-later");
const status = document.querySelector("#welcome-status");

const KEY = "rumurule.setup";

function setStatus(text, tone = "") {
  status.textContent = text;
  status.dataset.tone = tone;
}

function setBusy(busy) {
  setup.disabled = busy;
  later.disabled = busy;
  setup.textContent = busy ? "Waiting for approval…" : "Set up once";
}

async function runSetup() {
  setBusy(true);
  setStatus("The system will ask for your password once.");
  try {
    await invoke("helper_install");
    localStorage.setItem(KEY, "done");
    setStatus("Installed. The tunnel will start without a prompt.");
    setup.textContent = "Ready";
    appendLog("setup: privileged helper installed");
    setTimeout(() => {
      root.hidden = true;
      setBusy(false);
      setStatus("");
    }, 1100);
  } catch (error) {
    setStatus(String(error), "error");
    appendLog(`setup error: ${error}`);
    setBusy(false);
  }
}

export async function initWelcome() {
  if (!root) return;
  setup.addEventListener("click", runSetup);
  later.addEventListener("click", () => {
    localStorage.setItem(KEY, "later");
    root.hidden = true;
  });

  const skip = localStorage.getItem(KEY);
  if (skip === "done" || skip === "later") return;

  let state = null;
  try {
    state = await invoke("setup_state");
  } catch (error) {
    appendLog(`setup state error: ${error}`);
  }
  // A previous install already covers this user, or another one installed it.
  if (state?.installed && state.userMatches) {
    localStorage.setItem(KEY, "done");
    return;
  }

  if (state) {
    if (!state.polkit) {
      setStatus("pkexec is missing: install polkit to use the tunnel.", "error");
    } else if (!state.xray) {
      setStatus("The Xray binary is missing; the tunnel cannot start.", "error");
    }
  }
  root.hidden = false;
}

/** Reopen the setup from Settings, even after it was skipped. */
export async function openWelcome() {
  if (!root) return;
  setBusy(false);
  setStatus("");
  root.hidden = false;
  try {
    const state = await invoke("setup_state");
    if (state?.installed && state.userMatches) {
      setStatus("Already set up. Run it again to update the helper.");
    }
  } catch {
    // The install button still works; the check is only informational.
  }
}
