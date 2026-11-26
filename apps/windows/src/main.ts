import "@fontsource/space-grotesk/400.css";
import "@fontsource/space-grotesk/500.css";
import "@fontsource/space-grotesk/700.css";
import "@fontsource/doto/700.css";
import "@fontsource/doto/900.css";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";

mount(App, { target: document.getElementById("app")! });

// Toute erreur inattendue est signalée au lieu de figer silencieusement l'interface.
import { app } from "./lib/state.svelte";
window.addEventListener("error", (e) => app.toast(`Erreur d'interface : ${e.message}`, "error"));
window.addEventListener("unhandledrejection", (e) => app.toast(`Erreur : ${String(e.reason)}`, "error"));
