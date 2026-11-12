// Xray log panel.

const logBox = document.querySelector("#log");

/** Append a line, keeping the view pinned to the bottom when it already is. */
export function appendLog(line) {
  const atBottom = logBox.scrollTop + logBox.clientHeight >= logBox.scrollHeight - 24;
  logBox.textContent += `${line}\n`;
  if (atBottom) logBox.scrollTop = logBox.scrollHeight;
}

// Show the newest lines when the log tab becomes visible.
document.addEventListener("viewchange", (event) => {
  if (event.detail === "logs") logBox.scrollTop = logBox.scrollHeight;
});
