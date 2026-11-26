package io.github.nabeiroriviereadriano.nothingconnect.service

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import io.github.nabeiroriviereadriano.nothingconnect.core.Core

/** Relance la connexion après redémarrage du téléphone ou mise à jour de l'application. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        if (Core.prefs.serviceEnabled && Core.devices.value.isNotEmpty()) {
            runCatching { ConnectionService.start(ctx) }
        }
    }
}
