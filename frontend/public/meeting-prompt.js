// Fork-only (bendfeldt/meetily): buttons of the Teams meeting pop-up.
// The panel is shown and hidden from Rust (meeting_detector.rs).
const respond = (record) =>
  window.__TAURI_INTERNALS__.invoke('meeting_prompt_respond', { record });

document.getElementById('record').addEventListener('click', () => respond(true));
document.getElementById('dismiss').addEventListener('click', () => respond(false));
