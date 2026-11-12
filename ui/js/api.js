// Thin bridge to the Rust side.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

export { invoke, listen };
