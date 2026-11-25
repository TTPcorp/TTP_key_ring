import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";
import { applyTheme, loadTheme } from "./theme";

// Before mounting, so the page never flashes in the wrong theme.
applyTheme(loadTheme());

const target = document.getElementById("app");
if (!target) {
  throw new Error("missing #app element");
}

export default mount(App, { target });
