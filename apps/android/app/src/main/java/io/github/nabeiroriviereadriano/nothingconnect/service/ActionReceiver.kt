package io.github.nabeiroriviereadriano.nothingconnect.service

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import io.github.nabeiroriviereadriano.nothingconnect.core.Notifs

/** Boutons des notifications : accepter/refuser, pause/reprise/annulation, arrêt du service. */
class ActionReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val id = intent.getStringExtra(EXTRA) ?: ""
        when (intent.action) {
            ACCEPT -> { Core.call { Core.node.acceptTransfer(id, true) }; Notifs.clearIncoming(ctx, id) }
            REJECT -> { Core.call { Core.node.acceptTransfer(id, false) }; Notifs.clearIncoming(ctx, id) }
            PAUSE -> Core.call { Core.node.pauseTransfer(id) }
            RESUME -> Core.call { Core.node.resumeTransfer(id) }
            CANCEL -> Core.call { Core.node.cancelTransfer(id) }
            STOP_SERVICE -> {
                Core.prefs.serviceEnabled = false
                ConnectionService.stop(ctx)
            }
        }
    }

    companion object {
        const val EXTRA = "id"
        const val ACCEPT = "nc.ACCEPT"
        const val REJECT = "nc.REJECT"
        const val PAUSE = "nc.PAUSE"
        const val RESUME = "nc.RESUME"
        const val CANCEL = "nc.CANCEL"
        const val STOP_SERVICE = "nc.STOP"
    }
}
