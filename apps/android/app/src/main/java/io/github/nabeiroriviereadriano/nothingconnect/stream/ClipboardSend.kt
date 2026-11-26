package io.github.nabeiroriviereadriano.nothingconnect.stream

import android.app.PendingIntent
import android.content.ClipboardManager
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import android.widget.Toast
import androidx.activity.ComponentActivity
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import org.json.JSONObject

/**
 * Envoi du presse-papiers du téléphone au PC. Android 10+ n'autorise la lecture du
 * presse-papiers qu'à l'application au premier plan : cette fenêtre invisible s'ouvre,
 * lit le contenu dès qu'elle a le focus, l'envoie et se ferme.
 */
class ClipboardSendActivity : ComponentActivity() {
    private var done = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Core.start()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus || done) return
        done = true
        val text = getSystemService(ClipboardManager::class.java).primaryClip
            ?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(this)?.toString()
        val msg = when {
            text.isNullOrEmpty() -> "Le presse-papiers est vide ou ne contient pas de texte."
            text.length > 1_000_000 -> "Texte trop long pour le presse-papiers (1 Mo maximum)."
            else -> {
                val n = Core.broadcast(JSONObject().put("t", "clipboard").put("content", text))
                if (n > 0) "Presse-papiers envoyé au PC" else "Aucun PC connecté"
            }
        }
        Toast.makeText(this, msg, Toast.LENGTH_SHORT).show()
        finish()
    }
}

/** Tuile des réglages rapides « Presse-papiers → PC ». */
class ClipboardTile : TileService() {
    override fun onStartListening() {
        qsTile?.apply {
            state = if (Core.connected.value.isNotEmpty()) Tile.STATE_INACTIVE else Tile.STATE_UNAVAILABLE
            label = "Presse-papiers → PC"
            updateTile()
        }
    }

    override fun onClick() {
        val i = Intent(this, ClipboardSendActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (Build.VERSION.SDK_INT >= 34) {
            startActivityAndCollapse(PendingIntent.getActivity(this, 0, i, PendingIntent.FLAG_IMMUTABLE))
        } else {
            @Suppress("DEPRECATION", "StartActivityAndCollapseDeprecated")
            startActivityAndCollapse(i)
        }
    }
}
