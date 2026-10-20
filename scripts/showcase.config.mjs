export default {
  key: "mines-sm",
  serve: { dir: "." },
  viewport: { width: 1280, height: 800 },
  durationMs: 30000,
  settleMs: 1200,

  async ready(page) {
    await page.waitForSelector("#auto-win", { timeout: 30000 });
    await page.waitForTimeout(800);
  },

  beats: [
    {
      label: "Auto-play a winning line",
      action: { click: "#auto-win" },
      wait: 2200,
      shot: "winning-board",
    },
    {
      label: "Observation and action mask for an agent",
      action: { scrollToSelector: "#obs" },
      wait: 1800,
      shot: "observation-action-mask",
      thumb: true,
    },
    {
      label: "Auto-play a losing line",
      action: [
        { scrollTo: 0 },
        { click: "#new" },
        { wait: 400 },
        { click: "#auto-lose" },
      ],
      wait: 2200,
      shot: "losing-board",
    },
    {
      label: "Conformance checks against the Rust engine",
      action: { click: "#run-checks" },
      wait: 2200,
      shot: "conformance-checks",
    },
    {
      label: "A fresh deterministic board",
      action: [
        { scrollTo: 0 },
        { click: "#new" },
      ],
      wait: 1500,
      shot: "fresh-board",
    },
  ],

  thumbFrom: "observation-action-mask",
};
