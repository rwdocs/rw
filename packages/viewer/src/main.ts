import "./fonts.css";
import "./app.css";
import App from "./App.svelte";
import { mount } from "svelte";

function syncDarkMode() {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  const apply = () => {
    document.documentElement.classList.toggle("dark", mq.matches);
  };
  apply();
  mq.addEventListener("change", apply);
}

syncDarkMode();

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
