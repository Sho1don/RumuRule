// Floating menu and the rename dialog.

const menuRoot = document.querySelector("#menu");
const modal = document.querySelector("#modal");
const modalForm = document.querySelector("#modal-form");
const modalTitle = document.querySelector("#modal-title");
const modalInput = document.querySelector("#modal-input");
const modalCancel = document.querySelector("#modal-cancel");

let fine = false;

function closeMenu() {
  menuRoot.hidden = true;
  menuRoot.textContent = "";
}

function placeMenu(anchor) {
  const rect = anchor.getBoundingClientRect();
  const width = menuRoot.offsetWidth;
  const height = menuRoot.offsetHeight;
  const left = Math.max(8, Math.min(rect.right - width, window.innerWidth - width - 8));
  let top = rect.bottom + 4;
  if (top + height > window.innerHeight - 8) {
    top = Math.max(8, rect.top - height - 4);
  }
  menuRoot.style.left = `${left}px`;
  menuRoot.style.top = `${top}px`;
}

/** Show a menu next to `anchor`; `onPick` receives the chosen action. */
export function openMenu(anchor, items, onPick) {
  menuRoot.textContent = "";
  for (const item of items) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = item.danger ? "menu-item danger" : "menu-item";
    button.setAttribute("role", "menuitem");
    button.textContent = item.label;
    button.addEventListener("click", () => {
      closeMenu();
      onPick(item.action);
    });
    menuRoot.append(button);
  }

  menuRoot.hidden = false;
  placeMenu(anchor);
}

/** Ask for a single line of text; resolves to `null` when cancelled. */
export function askName({ title, label, value }) {
  modalTitle.textContent = title;
  document.querySelector("#modal-label").textContent = label;
  modalInput.value = value;
  modal.hidden = false;
  modalInput.focus();
  modalInput.select();

  return new Promise((resolve) => {
    fine = (name) => {
      modal.hidden = true;
      fine = false;
      resolve(name);
    };
  });
}

function cancel() {
  if (fine) fine(null);
}

modalForm.addEventListener("submit", (event) => {
  event.preventDefault();
  const name = modalInput.value.trim();
  if (!name) return;
  if (fine) fine(name);
});

modalCancel.addEventListener("click", cancel);

modal.addEventListener("click", (event) => {
  if (event.target === modal) cancel();
});

window.addEventListener(
  "pointerdown",
  (event) => {
    if (!menuRoot.hidden && !menuRoot.contains(event.target)) closeMenu();
  },
  true,
);

window.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  closeMenu();
  cancel();
});

window.addEventListener("resize", closeMenu);
window.addEventListener("blur", closeMenu);
