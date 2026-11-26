import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import { app } from "./state.svelte";

/** Ouvre le sélecteur Windows puis envoie au téléphone sélectionné. */
export async function pickAndSend(folder: boolean) {
  if (!app.selected || !app.online) {
    app.toast("Connectez d'abord un téléphone.", "error");
    return;
  }
  const picked = await open({ multiple: true, directory: folder, title: folder ? "Choisir des dossiers à envoyer" : "Choisir des fichiers à envoyer" });
  if (!picked) return;
  const paths = Array.isArray(picked) ? picked : [picked];
  if (paths.length) await app.run(api.sendPaths(app.selected, paths));
}
